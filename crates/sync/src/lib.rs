//! Native multi-room synchronization service.
//!
//! Each enabled instance binds a LAN-only iroh (QUIC) endpoint, advertises
//! itself via mDNS and watches for other rsplayer instances. A leader dials
//! followers by their `EndpointId`, keeps a control stream per follower and
//! (in later phases) fans out timestamped PCM. A follower accepts a single
//! leader connection, locks its local transport controls and plays what it
//! is told.

pub mod clock;
pub mod endpoint;
pub mod follower;
pub mod leader;
pub mod protocol;

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, bail};
use futures::StreamExt;
use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointAddr, EndpointId};
use iroh_mdns_address_lookup::DiscoveryEvent;
use log::{info, warn};
use tokio::select;
use tokio::sync::{broadcast, mpsc};

use api_models::common::MultiroomCommand;
use api_models::settings::MultiroomSettings;
use api_models::state::{MultiroomGroupState, MultiroomPeer, MultiroomRole, StateChangeEvent};
use playback::rsp::player_service::PlayerService;
use playback::rsp::tee::TeeEvent;

use crate::leader::{AUDIO_BROADCAST_CAPACITY, AudioMsg};
use crate::protocol::{ALPN, ControlToFollower, ControlToLeader, MAX_CONTROL_FRAME_BYTES, PROTOCOL_VERSION, read_frame, write_frame};

/// Everything the sync service needs from the composition root.
pub struct SyncDeps {
    pub settings: MultiroomSettings,
    pub db: Arc<fjall::Database>,
    pub state_changes_tx: broadcast::Sender<StateChangeEvent>,
    pub commands_rx: mpsc::Receiver<MultiroomCommand>,
    pub player_service: Arc<PlayerService>,
    /// Set while this instance is a grouped follower; the server rejects
    /// local transport commands when it is true.
    pub follower_active: Arc<AtomicBool>,
    /// PCM events from the playback thread (leader role).
    pub tee_rx: mpsc::Receiver<TeeEvent>,
    /// Shared with the playback thread: true while ≥1 follower is grouped.
    pub tee_active: Arc<AtomicBool>,
    /// Parameters for opening the local output when playing as a follower.
    pub sink_params: follower::SinkParams,
}

struct Peer {
    room_name: String,
    online: bool,
    /// Dial target for manually registered peers (mDNS fallback).
    manual_addr: Option<EndpointAddr>,
    /// Addresses from the last mDNS announcement. Dialing these directly is
    /// more reliable than a bare-id dial, which needs an active mDNS query
    /// at connect time that some networks answer only intermittently.
    discovered_addr: Option<EndpointAddr>,
}

struct FollowerHandle {
    room_name: String,
    conn: Connection,
    to_follower_tx: mpsc::Sender<ControlToFollower>,
}

enum Role {
    Idle,
    Leader { followers: BTreeMap<String, FollowerHandle> },
    Follower { leader_name: String, conn: Connection },
}

/// Internal events produced by connection/discovery tasks for the main loop.
pub(crate) enum Event {
    Discovered {
        id: String,
        room_name: Option<String>,
        addr: Option<EndpointAddr>,
    },
    Expired { id: String },
    FollowerConnected(Box<FollowerConnected>),
    FollowerConnectFailed { id: String, error: String },
    FollowerGone { id: String },
    LeaderHello(Box<LeaderHello>),
    LeaderGroupState(MultiroomGroupState),
    LeaderGone,
}

struct FollowerConnected {
    id: String,
    room_name: String,
    conn: Connection,
    to_follower_tx: mpsc::Sender<ControlToFollower>,
}

struct LeaderHello {
    leader_name: String,
    conn: Connection,
}

pub async fn run_sync_service(deps: SyncDeps) {
    if let Err(e) = run_inner(deps).await {
        log::error!("Multiroom sync service failed: {e:#}");
    }
}

async fn run_inner(mut deps: SyncDeps) -> Result<()> {
    let sync_endpoint = endpoint::bind(&deps.db, &deps.settings.room_name).await?;
    let endpoint = sync_endpoint.endpoint;
    let mut discovery = sync_endpoint.mdns.subscribe().await;

    let (events_tx, mut events_rx) = mpsc::channel::<Event>(64);
    // Handshake tasks consult this so an instance never acks a leader while
    // it is already grouped (small races are resolved by the main loop).
    let busy = Arc::new(AtomicBool::new(false));

    // Leader-side audio pipeline: playback tee → timestamped chunks → one
    // broadcast fan-out consumed by each follower's writer task.
    let (audio_tx, _) = broadcast::channel::<AudioMsg>(AUDIO_BROADCAST_CAPACITY);
    let ingestion = leader::spawn_tee_ingestion(deps.tee_rx, deps.state_changes_tx.subscribe(), audio_tx.clone());
    // Song progress mirrored to followers from the leader's own events.
    let mut state_rx = deps.state_changes_tx.subscribe();

    let mut service = Service {
        endpoint: endpoint.clone(),
        settings: deps.settings,
        state_changes_tx: deps.state_changes_tx,
        player: deps.player_service,
        follower_active: deps.follower_active,
        tee_active: deps.tee_active,
        audio_tx,
        sink_params: deps.sink_params,
        busy: busy.clone(),
        events_tx: events_tx.clone(),
        peers: BTreeMap::new(),
        role: Role::Idle,
    };
    service.emit_group_event();

    loop {
        select! {
            cmd = deps.commands_rx.recv() => {
                let Some(cmd) = cmd else {
                    info!("Multiroom command channel closed, stopping sync service.");
                    break;
                };
                service.handle_command(cmd);
            }
            event = events_rx.recv() => {
                if let Some(event) = event {
                    service.handle_event(event);
                }
            }
            discovered = discovery.next() => {
                let Some(discovered) = discovered else {
                    warn!("mDNS discovery stream ended.");
                    continue;
                };
                service.handle_discovery(&discovered);
            }
            state_event = state_rx.recv() => {
                if let Ok(StateChangeEvent::SongTimeEvent(progress)) = state_event {
                    service.forward_progress(&progress);
                }
            }
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else {
                    warn!("Multiroom endpoint closed.");
                    break;
                };
                let events_tx = events_tx.clone();
                let busy = busy.clone();
                let room_name = service.settings.room_name.clone();
                let sink_params = service.sink_params.clone();
                tokio::spawn(async move {
                    if let Err(e) = accept_leader(incoming, &room_name, &busy, &events_tx, sink_params).await {
                        info!("Incoming multiroom connection rejected: {e:#}");
                    }
                });
            }
        }
    }
    ingestion.abort();
    endpoint.close().await;
    Ok(())
}

struct Service {
    endpoint: Endpoint,
    settings: MultiroomSettings,
    state_changes_tx: broadcast::Sender<StateChangeEvent>,
    player: Arc<PlayerService>,
    follower_active: Arc<AtomicBool>,
    /// Shared with the playback thread — true while ≥1 follower is grouped.
    tee_active: Arc<AtomicBool>,
    audio_tx: broadcast::Sender<AudioMsg>,
    sink_params: follower::SinkParams,
    busy: Arc<AtomicBool>,
    events_tx: mpsc::Sender<Event>,
    peers: BTreeMap<String, Peer>,
    role: Role,
}

impl Service {
    fn handle_command(&mut self, cmd: MultiroomCommand) {
        match cmd {
            MultiroomCommand::QueryState => {
                self.emit_peers_event();
                self.emit_group_event();
            }
            MultiroomCommand::AddToGroup(id) => self.add_to_group(&id),
            MultiroomCommand::RemoveFromGroup(id) => self.remove_from_group(&id, "removed from group by leader"),
            MultiroomCommand::LeaveGroup => self.leave_group(),
            MultiroomCommand::AddManualPeer(spec) => {
                match parse_manual_peer(&spec) {
                    Ok((id, addr)) => {
                        self.peers
                            .entry(id.to_string())
                            .or_insert_with(|| Peer {
                                room_name: short_id(&id.to_string()),
                                online: true,
                                manual_addr: None,
                                discovered_addr: None,
                            })
                            .manual_addr = Some(addr);
                        self.emit_peers_event();
                    }
                    Err(e) => self.notify_error(&format!("Invalid peer address '{spec}': {e}")),
                }
            }
        }
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Discovered { id, room_name, addr } => {
                if id == self.endpoint.id().to_string() {
                    return;
                }
                // mDNS re-announces periodically; only notify the UI when
                // something actually changed.
                let mut changed = !self.peers.contains_key(&id);
                let peer = self.peers.entry(id.clone()).or_insert_with(|| Peer {
                    room_name: short_id(&id),
                    online: true,
                    manual_addr: None,
                    discovered_addr: None,
                });
                changed |= !peer.online;
                peer.online = true;
                if addr.is_some() {
                    peer.discovered_addr = addr;
                }
                if let Some(name) = room_name
                    && peer.room_name != name
                {
                    peer.room_name = name;
                    changed = true;
                }
                if changed {
                    self.emit_peers_event();
                }
            }
            Event::Expired { id } => {
                if let Some(peer) = self.peers.get_mut(&id)
                    && peer.online
                {
                    peer.online = false;
                    self.emit_peers_event();
                }
            }
            Event::FollowerConnected(connected) => self.on_follower_connected(*connected),
            Event::FollowerConnectFailed { id, error } => {
                let name = self.peer_name(&id);
                self.notify_error(&format!("Failed to add '{name}' to the group: {error}"));
            }
            Event::FollowerGone { id } => self.remove_from_group(&id, "follower disconnected"),
            Event::LeaderHello(hello) => self.on_leader_hello(*hello),
            Event::LeaderGroupState(mut group) => {
                if let Role::Follower { leader_name, .. } = &self.role {
                    group.role = MultiroomRole::Follower;
                    group.leader_name = Some(leader_name.clone());
                    let _ = self.state_changes_tx.send(StateChangeEvent::MultiroomGroupEvent(group));
                }
            }
            Event::LeaderGone => {
                if matches!(self.role, Role::Follower { .. }) {
                    info!("Leader connection lost, leaving group.");
                    self.leave_group();
                }
            }
        }
    }

    fn handle_discovery(&mut self, event: &DiscoveryEvent) {
        match event {
            DiscoveryEvent::Discovered { endpoint_info, .. } => {
                let id = endpoint_info.endpoint_id.to_string();
                let room_name = endpoint_info.data.user_data().map(ToString::to_string);
                let addr = endpoint_info.to_endpoint_addr();
                let addr = (!addr.addrs.is_empty()).then_some(addr);
                self.handle_event(Event::Discovered { id, room_name, addr });
            }
            DiscoveryEvent::Expired { endpoint_id, .. } => {
                self.handle_event(Event::Expired {
                    id: endpoint_id.to_string(),
                });
            }
            _ => {}
        }
    }

    fn add_to_group(&self, id: &str) {
        if matches!(self.role, Role::Follower { .. }) {
            self.notify_error("This instance is a follower and cannot lead a group. Leave the group first.");
            return;
        }
        if let Role::Leader { followers } = &self.role
            && followers.contains_key(id)
        {
            return;
        }
        let addr = match self.dial_addr(id) {
            Ok(addr) => addr,
            Err(e) => {
                self.notify_error(&format!("Invalid endpoint id '{id}': {e}"));
                return;
            }
        };
        let endpoint = self.endpoint.clone();
        let events_tx = self.events_tx.clone();
        let our_name = self.settings.room_name.clone();
        let id = id.to_string();
        tokio::spawn(async move {
            // Bounded dial: without it a failed mDNS resolution leaves the
            // user staring at a toggle that never reacts.
            let dial = tokio::time::timeout(
                std::time::Duration::from_secs(12),
                connect_to_follower(&endpoint, addr, &our_name),
            )
            .await
            .unwrap_or_else(|_| Err(anyhow::anyhow!("connection timed out — peer offline or unreachable?")));
            match dial {
                Ok((connected, mut recv)) => {
                    let connected = FollowerConnected { id: id.clone(), ..connected };
                    let _ = events_tx.send(Event::FollowerConnected(Box::new(connected))).await;
                    // Read the follower's control messages until it leaves
                    // or the connection dies.
                    loop {
                        match read_frame::<ControlToLeader>(&mut recv, MAX_CONTROL_FRAME_BYTES).await {
                            Ok(ControlToLeader::LeaveGroup) | Err(_) => {
                                let _ = events_tx.send(Event::FollowerGone { id }).await;
                                break;
                            }
                            Ok(_) => {}
                        }
                    }
                }
                Err(e) => {
                    let _ = events_tx
                        .send(Event::FollowerConnectFailed {
                            id,
                            error: format!("{e:#}"),
                        })
                        .await;
                }
            }
        });
    }

    fn on_follower_connected(&mut self, connected: FollowerConnected) {
        let FollowerConnected {
            id,
            room_name,
            conn,
            to_follower_tx,
        } = connected;
        if let Some(peer) = self.peers.get_mut(&id) {
            peer.room_name.clone_from(&room_name);
            peer.online = true;
        }
        let handle = FollowerHandle {
            room_name: room_name.clone(),
            conn: conn.clone(),
            to_follower_tx: to_follower_tx.clone(),
        };
        match &mut self.role {
            Role::Leader { followers } => {
                followers.insert(id, handle);
            }
            Role::Idle => {
                let mut followers = BTreeMap::new();
                followers.insert(id, handle);
                self.role = Role::Leader { followers };
                self.busy.store(true, Ordering::SeqCst);
            }
            Role::Follower { .. } => {
                // Race: we became a follower while the dial was in flight.
                handle.conn.close(0u32.into(), b"not a leader anymore");
                return;
            }
        }
        // Audio fan-out and clock responder live for the connection.
        tokio::spawn(leader::run_follower_audio_writer(conn.clone(), self.audio_tx.subscribe(), to_follower_tx));
        tokio::spawn(leader::run_clock_responder(conn));
        self.tee_active.store(true, Ordering::SeqCst);
        self.notify_success(&format!("'{room_name}' joined the group"));
        self.emit_peers_event();
        self.broadcast_group_state();
    }

    fn remove_from_group(&mut self, id: &str, reason: &str) {
        let Role::Leader { followers } = &mut self.role else {
            return;
        };
        let Some(handle) = followers.remove(id) else {
            return;
        };
        handle.conn.close(0u32.into(), reason.as_bytes());
        self.notify_success(&format!("'{}' left the group", handle.room_name));
        if let Role::Leader { followers } = &self.role
            && followers.is_empty()
        {
            self.role = Role::Idle;
            self.busy.store(false, Ordering::SeqCst);
            self.tee_active.store(false, Ordering::SeqCst);
        }
        self.emit_peers_event();
        self.broadcast_group_state();
    }

    /// Mirrors the leader's song progress to all followers.
    fn forward_progress(&self, progress: &api_models::state::SongProgress) {
        if let Role::Leader { followers } = &self.role {
            for handle in followers.values() {
                let _ = handle.to_follower_tx.try_send(ControlToFollower::SongProgress {
                    current_secs: progress.current_time.as_secs(),
                    total_secs: progress.total_time.as_secs(),
                });
            }
        }
    }

    fn on_leader_hello(&mut self, hello: LeaderHello) {
        let LeaderHello { leader_name, conn } = hello;
        if !matches!(self.role, Role::Idle) {
            conn.close(1u32.into(), b"already grouped");
            return;
        }
        info!("Joined multiroom group led by '{leader_name}'.");
        self.busy.store(true, Ordering::SeqCst);
        self.follower_active.store(true, Ordering::SeqCst);
        self.player.stop_current_song();
        self.role = Role::Follower { leader_name, conn };
        self.emit_group_event();
    }

    fn leave_group(&mut self) {
        match std::mem::replace(&mut self.role, Role::Idle) {
            Role::Follower { conn, .. } => {
                conn.close(0u32.into(), b"left group");
            }
            Role::Leader { followers } => {
                for handle in followers.into_values() {
                    handle.conn.close(0u32.into(), b"group disbanded");
                }
            }
            Role::Idle => {}
        }
        self.busy.store(false, Ordering::SeqCst);
        self.follower_active.store(false, Ordering::SeqCst);
        self.tee_active.store(false, Ordering::SeqCst);
        self.emit_peers_event();
        self.emit_group_event();
    }

    fn dial_addr(&self, id: &str) -> Result<EndpointAddr> {
        if let Some(peer) = self.peers.get(id) {
            if let Some(addr) = &peer.manual_addr {
                return Ok(addr.clone());
            }
            if let Some(addr) = &peer.discovered_addr {
                return Ok(addr.clone());
            }
        }
        Ok(EndpointAddr::from(EndpointId::from_str(id)?))
    }

    fn peer_name(&self, id: &str) -> String {
        self.peers.get(id).map_or_else(|| short_id(id), |p| p.room_name.clone())
    }

    fn group_members(&self) -> Vec<MultiroomPeer> {
        match &self.role {
            Role::Leader { followers } => followers
                .iter()
                .map(|(id, handle)| MultiroomPeer {
                    endpoint_id: id.clone(),
                    room_name: handle.room_name.clone(),
                    in_group: true,
                    online: true,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn group_state(&self) -> MultiroomGroupState {
        let (role, leader_name) = match &self.role {
            Role::Idle => (MultiroomRole::Idle, None),
            Role::Leader { .. } => (MultiroomRole::Leader, Some(self.settings.room_name.clone())),
            Role::Follower { leader_name, .. } => (MultiroomRole::Follower, Some(leader_name.clone())),
        };
        MultiroomGroupState {
            role,
            leader_name,
            members: self.group_members(),
        }
    }

    fn broadcast_group_state(&self) {
        self.emit_group_event();
        let state = self.group_state();
        if let Role::Leader { followers } = &self.role {
            for handle in followers.values() {
                let _ = handle.to_follower_tx.try_send(ControlToFollower::GroupState(state.clone()));
            }
        }
    }

    fn emit_group_event(&self) {
        let mut state = self.group_state();
        if !self.settings.enabled {
            state.role = MultiroomRole::Off;
        }
        let _ = self.state_changes_tx.send(StateChangeEvent::MultiroomGroupEvent(state));
    }

    fn emit_peers_event(&self) {
        let in_group = |id: &str| match &self.role {
            Role::Leader { followers } => followers.contains_key(id),
            _ => false,
        };
        let peers = self
            .peers
            .iter()
            .map(|(id, peer)| MultiroomPeer {
                endpoint_id: id.clone(),
                room_name: peer.room_name.clone(),
                in_group: in_group(id),
                online: peer.online,
            })
            .collect();
        let _ = self.state_changes_tx.send(StateChangeEvent::MultiroomPeersEvent(peers));
    }

    fn notify_success(&self, message: &str) {
        let _ = self
            .state_changes_tx
            .send(StateChangeEvent::NotificationSuccess(message.to_string()));
    }

    fn notify_error(&self, message: &str) {
        let _ = self
            .state_changes_tx
            .send(StateChangeEvent::NotificationError(message.to_string()));
    }
}

/// Dials a follower, performs the Hello/HelloAck handshake and spawns the
/// control stream writer task. Returns the connection handle plus the recv
/// half so the caller can watch for `LeaveGroup`/disconnect.
async fn connect_to_follower(
    endpoint: &Endpoint,
    addr: EndpointAddr,
    our_name: &str,
) -> Result<(FollowerConnected, iroh::endpoint::RecvStream)> {
    let conn = endpoint.connect(addr, ALPN).await.context("connect failed")?;
    let (mut send, mut recv) = conn.open_bi().await.context("failed to open control stream")?;
    write_frame(
        &mut send,
        &ControlToFollower::Hello {
            protocol_version: PROTOCOL_VERSION,
            leader_name: our_name.to_string(),
        },
    )
    .await?;
    let ack: ControlToLeader = read_frame(&mut recv, MAX_CONTROL_FRAME_BYTES).await?;
    let ControlToLeader::HelloAck {
        protocol_version,
        follower_name,
    } = ack
    else {
        bail!("unexpected handshake response");
    };
    if protocol_version != PROTOCOL_VERSION {
        bail!("protocol version mismatch: leader {PROTOCOL_VERSION}, follower {protocol_version}");
    }

    let (to_follower_tx, mut to_follower_rx) = mpsc::channel::<ControlToFollower>(16);
    tokio::spawn(async move {
        while let Some(msg) = to_follower_rx.recv().await {
            if write_frame(&mut send, &msg).await.is_err() {
                break;
            }
        }
    });
    Ok((
        FollowerConnected {
            id: String::new(), // filled in by the caller
            room_name: follower_name,
            conn,
            to_follower_tx,
        },
        recv,
    ))
}

/// Accepts an incoming leader connection on the follower side: handshake,
/// then hands the connection to [`follower::run_grouped_follower`] until
/// the leader disconnects.
async fn accept_leader(
    incoming: iroh::endpoint::Incoming,
    our_name: &str,
    busy: &Arc<AtomicBool>,
    events_tx: &mpsc::Sender<Event>,
    sink_params: follower::SinkParams,
) -> Result<()> {
    let conn = incoming.await.context("incoming connection failed")?;
    let (mut send, mut recv) = conn.accept_bi().await.context("failed to accept control stream")?;
    let hello: ControlToFollower = read_frame(&mut recv, MAX_CONTROL_FRAME_BYTES).await?;
    let ControlToFollower::Hello {
        protocol_version,
        leader_name,
    } = hello
    else {
        bail!("unexpected handshake message");
    };
    if protocol_version != PROTOCOL_VERSION {
        conn.close(1u32.into(), b"protocol version mismatch");
        bail!("protocol version mismatch: leader {protocol_version}, this instance {PROTOCOL_VERSION}");
    }
    if busy.load(Ordering::SeqCst) {
        conn.close(1u32.into(), b"already grouped");
        bail!("rejected leader '{leader_name}': already grouped");
    }
    write_frame(
        &mut send,
        &ControlToLeader::HelloAck {
            protocol_version: PROTOCOL_VERSION,
            follower_name: our_name.to_string(),
        },
    )
    .await?;

    let _ = events_tx
        .send(Event::LeaderHello(Box::new(LeaderHello {
            leader_name,
            conn: conn.clone(),
        })))
        .await;

    // Everything after the handshake — control stream, clock sync, audio
    // sessions — runs here until the leader disconnects.
    follower::run_grouped_follower(conn, send, recv, sink_params, events_tx.clone()).await;
    let _ = events_tx.send(Event::LeaderGone).await;
    Ok(())
}

/// Parses `endpoint_id` or `endpoint_id@ip:port`.
fn parse_manual_peer(spec: &str) -> Result<(EndpointId, EndpointAddr)> {
    let (id_part, addr_part) = spec.split_once('@').map_or((spec, None), |(id, addr)| (id, Some(addr)));
    let id = EndpointId::from_str(id_part.trim()).context("invalid endpoint id")?;
    let mut addr = EndpointAddr::from(id);
    if let Some(sock) = addr_part {
        let sock: std::net::SocketAddr = sock.trim().parse().context("invalid ip:port")?;
        addr = addr.with_ip_addr(sock);
    }
    Ok((id, addr))
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}
