# rsplayer-pocket-dap

Small DAP front-panel UI for a running `rsplayer` (WebSocket `/api/ws`). Default backend is an X11/HDMI window; SPI hardware is not implemented yet.

## Quick test

1. Start `rsplayer` (default port `8000`, or set `PORT`).
2. From the repo root:

```bash
cargo run -p rsplayer-pocket-dap -- --display st7789 --profile mpod --show-controls
```

That opens a pixel-accurate ST7789 panel plus the mPod chassis (click wheel / volume). Point `--ws` at another host if needed:

```bash
cargo run -p rsplayer-pocket-dap -- --display st7789 --profile mpod --show-controls \
  --ws ws://127.0.0.1:8000/api/ws
```

## Useful flags

| Flag | Effect |
|------|--------|
| *(omit `--display`)* | ST7789, window scaled with FitScreen |
| `--display st7789\|st7567` | Fixed logical panel (integer buffer scale) |
| `--show-controls` | Draw chassis chrome (touch / mouse) |
| `--full-screen` | Borderless FitScreen |
| `--profile mpod\|creative\|pocket-dap\|playlist` | Behavior + default colors (Win Amp = `playlist`) |
| `--theme path.json` | Color override for the selected profile |
| `--backend window\|spi` | `window` (default); `spi` not implemented |

## Controls (with `--show-controls`)

| Input | Action |
|-------|--------|
| Wheel / `[` `]` / mouse wheel | Volume on Now Playing; list scroll elsewhere |
| Center / Enter / Space | Select |
| D-pad / wheel sectors | Navigate (mPod: North=Menu/back, South=play/pause) |
| `-` `=` | Volume |
| `M` | Menu (hold on long lists = letter jump) |
| Esc | Quit |

Screen flow: [docs/pocket_dap/views.md](../../docs/pocket_dap/views.md).
