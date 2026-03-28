# Usage Guide

This guide covers the main features and navigation of RSPlayer's web interface.

## Navigation

RSPlayer uses a single-page application with hash-based routing. The main sections are accessible via the navigation bar:

| Section | URL | Description |
|---------|-----|-------------|
| Player | `#/player` | Main playback controls and current track info |
| Queue | `#/queue` | Current playback queue |
| Playlists | `#/library/playlists` | Saved playlists |
| Library | `#/library/files` | Music library browser |
| Settings | `#/settings` | Configuration options |

### Keyboard Navigation

Press **?** anywhere to show the keyboard shortcuts help:

![Keyboard Shortcuts](/_assets/player_keyboard_shortcuts.png)

| Key | Action |
|-----|--------|
| 1 | Now Playing |
| 2 | Queue |
| 3 | Library |
| 4 | Settings |
| / | Focus search field |
| ? | Show/hide keyboard shortcuts |

## Player Page

The Player page is the main interface for controlling playback.

![Player Main](/_assets/player_main.png)

### Track Information

The top section displays information about the currently playing track:

- **Title** - Song title (clickable to search in library)
- **Artist** - Artist name (clickable to filter by artist)
- **Album** - Album name (clickable to search in library)
- **Genre** and **Date** - Additional metadata
- **Audio format** - Codec, bit depth, and sample rate (e.g., "FLAC - 24 / 96000 Hz")
- **Loudness** - LUFS value and normalization gain (when enabled)

### Playback Controls

| Control | Function |
|---------|----------|
| Play/Pause | Start or pause playback |
| Previous | Skip to previous track |
| Next | Skip to next track |
| Shuffle/Repeat | Cycle through: Sequential → Random → Loop Single → Loop Queue |
| Heart | Like/unlike the current track |
| Lyrics | Open synchronized lyrics modal |

### Progress Bar

- Drag to seek within the track
- Current time and total duration displayed
- Hover to see seek position tooltip

### Volume Control

- Mute/unmute button
- Volume down/up buttons
- Slider for precise volume adjustment
- Percentage display

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| Space | Play / Pause |
| ← / → | Previous / Next track |
| Shift+← / → | Seek back / forward 10s |
| ↑ / ↓ | Volume up / down |
| M | Mute / Unmute |
| L | Like / Unlike track |
| Y | Toggle lyrics |
| S | Shuffle / Repeat mode |
| Esc | Close modal |

### Synchronized Lyrics

RSPlayer integrates with LRCLIB for synchronized lyrics.

![Lyrics Modal](/_assets/player_lyrics.png)

1. Press **Y** or click the lyrics button on the player
2. Lyrics sync automatically with playback
3. Current line is highlighted
4. Plain lyrics shown if synchronized version not available

## Queue Page

The Queue page shows all tracks in the current playback queue.

![Queue](/_assets/player_queue.png)

### Queue Actions

| Action | Description |
|--------|-------------|
| Search | Find songs in the queue |
| Add URL | Add streaming URL(s) to queue |
| Save | Save queue as a playlist |
| Focus | Show queue starting from current song |
| Clear | Remove all items from queue |

### Queue Item Actions

Each song in the queue has a menu with:

- **Play Next** - Move song to play after current track
- **Play** - Start playing this song immediately
- **Remove** - Remove from queue

### Drag and Drop

Reorder queue items by dragging the handle (⠿) on the left side of each item.

## Playlists Page

Manage your saved playlists and auto-generated collections.

![Playlists](/_assets/player_playlists.png)

### Playlist Types

- **User Playlists** - Playlists you create and save
- **Albums** - Automatically detected albums from your library
- **Genres** - Auto-generated playlists by genre

### Playlist Actions

When you select a playlist, a modal appears with options:

- **Load** - Replace queue with playlist contents
- **Add to queue** - Append playlist to current queue
- **Add next** - Add playlist after current track

## Library Page

Browse your music collection with multiple views.

### Files View

Browse by directory structure.

![Library Files](/_assets/player_library.png)

- Navigate through your music directories
- Expand/collapse folders
- Add folders or individual files to queue
- Search across your entire library

### Artists View

Browse by artist name.

![Library Artists](/_assets/library_artists.png)

- Alphabetical artist listing
- Click artist to see their albums and songs
- Quick add to queue options

### Radio View

Manage internet radio streams.

![Library Radio](/_assets/library_radio.png)

- Add streaming URLs
- Organize favorite stations
- Play radio streams directly

### Stats View

View library statistics and loudness analysis progress.

![Library Stats](/_assets/library_stats.png)

- Total tracks, albums, artists
- Loudness analysis progress
- Storage usage

### Adding to Queue

Right-click or use the action menu on any item:

- **Add to queue** - Append to end of queue
- **Add next** - Add after current track
- **Play** - Replace queue and play immediately
- **Load** - Load directory contents to queue

## Settings Page

Configure RSPlayer settings. For detailed configuration options, see the [Configuration](configuration.md) page.

![Settings Overview](/_assets/settings_main.png)

### Quick Settings Reference

| Section | Description |
|---------|-------------|
| Appearance | Theme selection |
| Playback | Audio interface, volume control, auto-resume |
| Audio Processing | VU meter, loudness normalization, DSP |
| Music Library | Music directories, network storage |
| Hardware | USB firmware link, power control |

## Loudness Normalization

When enabled, RSPlayer automatically normalizes volume across tracks:

1. Enable in Settings → Audio Processing
2. Set target loudness (default: -18 LUFS)
3. Tracks are analyzed in background during idle
4. Progress visible on Library Stats page

The normalization preserves dynamic range while ensuring consistent perceived loudness across your library.
