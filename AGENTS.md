### Don't ever do
- git write commands: commit, push, merge, rebase ...

### Frontend
- frontend location is `rsplayer_web_ui/`, not a member of the main Rust workspace (WASM target)
- built with Dioxus framework, compiled to WebAssembly
- one-time setup after clone: `cd rsplayer_web_ui && npm install` (installs tailwind/daisyui/fontawesome/material-icons and copies font files to `public/`)
- compile check: after code change use `dx check`
- dev: `cd rsplayer_web_ui && dx serve` (hot-reload, proxies `/api` and `/artwork` to backend on localhost:8000)
- release: `cd rsplayer_web_ui && dx build --release --platform web` (output embedded in backend binary)
- CSS source is `rsplayer_web_ui/input.css`; compiled output is `rsplayer_web_ui/public/tw.css` (committed to git)
- regenerate CSS only when `input.css` changes: `npx tailwindcss -i input.css -o public/tw.css --minify`, then commit `public/tw.css`
- `public/fontawesome/` and `public/material-icons/` are gitignored — populated by `npm install`

### Backend
- Compile BE with `cargo make build_dev`
- Frontend release build must exist before compiling the backend (`dx build --release --platform web` embeds the UI)
- Run locally with `cargo make run_local`. CWD dir is `.run`
- When deployed to target env it is run by systemd defined in PGKS

### Documentation
- docsify site in `docs/`
- README.md
- Release notes are here `docs/release_notes.md`

### Versioning
- version is centralized in Makefile.toml and it is used by `rsplayer_backend/build.rs`

### Deploy to local test env (RPI)
- there are two test devices rpi zero and rpi4 (rpi_host and rpi_target env variables in Makefile.toml) it requires switch remove/add comment...
- If FE files changed: `cd rsplayer_web_ui && dx build --release --platform web && cd .. && cargo make copy_remote`
- Otherwise: `cargo make copy_remote`