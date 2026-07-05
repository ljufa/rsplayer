//! iroh endpoint construction: persisted identity, LAN-only configuration
//! and mDNS discovery.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use iroh::endpoint::{BindOpts, QuicTransportConfig, presets};
use iroh::{Endpoint, RelayMode, SecretKey};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use log::{info, warn};

use crate::protocol::ALPN;

const SECRET_KEY_KEY: &str = "secret_key";

/// How long a silent connection lives before it is declared dead. iroh's
/// 30s default leaves a killed/restarted follower checked in the leader's
/// group UI (and vice versa) for half a minute; on a LAN a few seconds is
/// plenty. Must comfortably exceed [`KEEP_ALIVE_INTERVAL`].
const CONNECTION_IDLE_TIMEOUT: Duration = Duration::from_secs(8);
/// Keep-alive ping cadence while a connection is otherwise silent
/// (grouped but not playing).
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(2);

/// Fixed UDP port for the sync endpoint, so a firewall can be opened for it
/// (e.g. `firewall-cmd --add-port=47800/udp`). An unsolicited inbound QUIC
/// dial — a peer initiating a group — never gets through a default-deny
/// firewall on a random ephemeral port.
pub const SYNC_UDP_PORT: u16 = 47800;

/// The bound iroh endpoint plus the mDNS lookup handle (kept for `subscribe`).
pub struct SyncEndpoint {
    pub endpoint: Endpoint,
    pub mdns: MdnsAddressLookup,
}

/// Binds the LAN-only iroh endpoint using the identity persisted in the
/// shared database, so the `EndpointId` survives restarts.
///
/// Prefers [`SYNC_UDP_PORT`]; when it is taken (typically a second instance
/// on the same machine) falls back to a random port, which still works on
/// open networks and for outbound-initiated groups.
pub async fn bind(db: &Arc<fjall::Database>, room_name: &str) -> Result<SyncEndpoint> {
    let secret_key = load_or_create_secret_key(db)?;

    let endpoint = match bind_endpoint(&secret_key, room_name, SYNC_UDP_PORT).await {
        Ok(endpoint) => endpoint,
        Err(e) => {
            warn!(
                "Failed to bind multiroom UDP port {SYNC_UDP_PORT} ({e:#}); using a random port — \
                 peers behind this machine's firewall exception will not reach it"
            );
            bind_endpoint(&secret_key, room_name, 0)
                .await
                .context("failed to bind iroh endpoint")?
        }
    };

    let mdns = MdnsAddressLookup::builder()
        .build(endpoint.id())
        .context("failed to start mDNS address lookup")?;
    endpoint
        .address_lookup()
        .context("endpoint has no address lookup registry")?
        .add(mdns.clone());

    info!(
        "Multiroom endpoint bound on {:?}. This instance's endpoint id: {}",
        endpoint.bound_sockets(),
        endpoint.id()
    );
    Ok(SyncEndpoint { endpoint, mdns })
}

async fn bind_endpoint(secret_key: &SecretKey, room_name: &str, port: u16) -> Result<Endpoint> {
    // Starts from iroh's tuned defaults (multipath, holepunching) and only
    // tightens dead-connection detection. The effective idle timeout of a
    // connection is the *minimum* of both peers' values, so old and new
    // versions interoperate.
    let transport_config = QuicTransportConfig::builder()
        .keep_alive_interval(KEEP_ALIVE_INTERVAL)
        .max_idle_timeout(Some(
            CONNECTION_IDLE_TIMEOUT.try_into().context("invalid idle timeout")?,
        ))
        .build();
    let mut builder = Endpoint::builder(presets::Minimal)
        .secret_key(secret_key.clone())
        .relay_mode(RelayMode::Disabled)
        .transport_config(transport_config)
        .alpns(vec![ALPN.to_vec()]);
    if port != 0 {
        builder = builder
            .bind_addr(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)))
            .context("invalid IPv4 bind address")?
            // IPv6 may be unavailable (matches iroh's own default v6 socket,
            // which is also allowed to fail).
            .bind_addr_with_opts(
                SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)),
                BindOpts::default().set_is_required(false),
            )
            .context("invalid IPv6 bind address")?;
    }
    // Publish the room name alongside the mDNS record so peers can show a
    // human-readable name without connecting first. Best effort: an invalid
    // name simply falls back to the Hello exchange.
    if let Ok(user_data) = room_name.parse() {
        builder = builder.user_data_for_address_lookup(user_data);
    }
    builder.bind().await.context("failed to bind iroh endpoint")
}

#[cfg(test)]
mod tests {
    use super::bind_endpoint;
    use iroh::SecretKey;

    #[tokio::test]
    async fn fixed_port_binds_and_conflicts_fall_back() {
        // Not SYNC_UDP_PORT itself: a dev instance may be running.
        let port = 47801;
        let first = bind_endpoint(&SecretKey::generate(), "room-a", port).await.unwrap();
        assert!(first.bound_sockets().iter().any(|s| s.port() == port));
        // Same port again must fail — this is what triggers the random-port
        // fallback in `bind` for a second instance on one machine.
        assert!(bind_endpoint(&SecretKey::generate(), "room-b", port).await.is_err());
        let fallback = bind_endpoint(&SecretKey::generate(), "room-b", 0).await.unwrap();
        assert!(fallback.bound_sockets().iter().all(|s| s.port() != port));
    }
}

fn load_or_create_secret_key(db: &Arc<fjall::Database>) -> Result<SecretKey> {
    let keyspace = db
        .keyspace("multiroom", fjall::KeyspaceCreateOptions::default)
        .context("failed to open multiroom keyspace")?;
    if let Some(bytes) = keyspace.get(SECRET_KEY_KEY)?
        && let Ok(raw) = <[u8; 32]>::try_from(bytes.as_ref())
    {
        return Ok(SecretKey::from_bytes(&raw));
    }
    let secret_key = SecretKey::generate();
    keyspace.insert(SECRET_KEY_KEY, secret_key.to_bytes())?;
    info!("Generated new multiroom identity.");
    Ok(secret_key)
}
