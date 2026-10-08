# Pocket DAP views

Screen flow for the `rsplayer-pocket-dap` window UI. Profiles may differ in their views; the shared idea stays this flow. Today, `--profile mpod` and `--profile creative` use the same screens and change which control moves between them. Behavior profiles are `mpod`, `creative`, `pocket-dap`, and `playlist`. Color skins such as `classic-green` and `lcd-ice` are JSON for `--theme` on `pocket-dap` first — not separate `--profile` names. Font and `seven_segment` stay in that JSON; there are no CLI flags for them. This page is only navigation.

Play, pause, skip, and volume never change the screen. Those controls are listed after the graphs.

## Reference

This file is the reference placeholder in the project. Do not spend time making every named DAP a faithful emulation. When a behavior from any player is worth keeping, write it here. It then lands on the pocket_dap surface.

The look of that surface is not decided. The first lean is mPod.

Shared modes are one abstraction. Profiles may differ in chassis and which control opens a screen. They do not fork the mode itself.

Letter input is one mode. While the player is picking characters for a search, that screen looks the same whether the query will match a song, an album, or an artist. There is no separate letter-input layout per result kind. The same rule applies to the other shared modes: song, album, and artist lists use one list, and the status line always reports the same kind of state.

On **Artists**, **All albums**, **Playlists**, and **All tracks**, the wheel scrolls the list. Hold Menu (or hold North on mPod) opens a letter picker; Center jumps to the first title with that letter. Fast wheel rotation enters a mPod-style letter scrub overlay and jumps the same way.

The screen shows the battery level. Where that value comes from is not decided. A service may pass it to the player through the filesystem, D-Bus, or another channel.

## Screens

| Screen | What it lists |
|--------|----------------|
| Now playing | Current song |
| Menu | Now Playing, Library, Queue, Settings |
| Settings | Not specified yet |
| Library | Artists, Albums, Playlists, Tracks, Search |
| Search | Query, then artist / album / song hits |
| Artists | One page of artists |
| Artist albums | Albums of the selected artist |
| All albums | Every album, by title |
| Playlists | Saved playlists |
| All tracks | One page of songs, by title |
| Tracks | Songs of one album, or of one playlist |
| Album actions | Play, Play Next, Add to queue |
| Track actions | Play, Play Next, Add to queue |
| Queue | Current queue |
| Queue item | Play, Play Next, Remove |

Album actions, track actions, and queue item are reachable on Creative only. On mPod, East is Next and does not open them. Settings is reference-only until implemented.

## mPod

North on Now playing opens the menu. North on every other screen goes back (see [Back](#back)). Center opens the highlighted row. East and West skip tracks, except while the search query is being edited: East moves onto the result list, West deletes a character (or leaves Search when the query is empty).

```text
Now playing
    | North (Menu)
    v
  Menu
    | Center
    +-- Now Playing
    +-- Library
    |     | Center
    |     +-- Artists      --> Artist albums -- Center --> Tracks -- Center --> Now playing
    |     +-- Albums --------------------------- Center --> Tracks
    |     +-- Playlists ------------------------ Center --> Tracks
    |     +-- Tracks --------------------------- Center ----------> Now playing
    |     +-- Search
    |           | Center
    |           +-- artist  --> Artist albums
    |           +-- album   --> Tracks
    |           +-- song    --> Now playing
    +-- Queue -- Center --> Now playing
    +-- Settings
```

The wheel changes volume on Now playing and scrolls the list everywhere else. South is play/pause on every screen. On Artists, All albums, Playlists, and All tracks: hold Menu/North for the letter picker; fast wheel for letter scrub.

## Creative

North opens the menu only on Now playing. On a list, North and South move the highlight (South is one wheel step). Center opens the row the same way as on mPod. East opens the row too on Menu, Library, Artists, and Playlists. On an album, a song, or a queue row, East opens the action screen instead of playing.

The side Play button (keyboard F1) is play/pause on every screen. The side Back button (keyboard F2) goes back. West goes back on every screen except Now playing (previous track) and Search (same delete / leave-editor behavior as mPod).

```text
Now playing
    | North
    v
  Menu
    | Center or East
    +-- Now Playing
    +-- Library
    |     | Center or East
    |     +-- Artists --> Artist albums -- Center --> Tracks -- Center --> Now playing
    |     |                  |                         |
    |     |                  +-- East --> Album actions +-- East --> Track actions
    |     +-- Albums -------------------- Center --> Tracks
    |     |                                  +-- East --> Album actions
    |     +-- Playlists ----------------- Center or East --> Tracks
    |     +-- Tracks -------------------- Center --> Now playing
    |     |                                  +-- East --> Track actions
    |     +-- Search
    |           | Center                         East (album or song)
    |           +-- artist  --> Artist albums
    |           +-- album   --> Tracks           --> Album actions
    |           +-- song    --> Now playing      --> Track actions
    +-- Queue -- Center --> Now playing
              +-- East --> Queue item
    +-- Settings
```

Play on an action screen starts that item and opens Now playing. The other action rows run and return to the list they came from. The wheel does nothing on Now playing; elsewhere it scrolls. Volume up/down (GPIO or the chassis buttons) only shows the volume bar. Letter picker / scrub on the four long library lists is the same as on mPod (hold Menu, or fast wheel).

## Back

| From | Returns to |
|------|------------|
| Menu, Now playing | Now playing |
| Library, Queue, Settings | Menu |
| Search | Library |
| Artists, All albums, Playlists, All tracks | Library |
| Artist albums | Artists, or Search if the artist was opened from Search |
| Tracks, Album actions | The list that opened them (artist albums, all albums, playlists, all tracks, or Search) |
| Track actions | All tracks, or the album/playlist Tracks list |
| Queue item | Queue |

mPod uses North. Creative uses the side Back button, and West on any screen that is not Now playing or Search.
