### Don't ever do
- git write commands: commit, push, merge, rebase ...

### Frontend
- frontend location is `web-ui/`, a member of the main Rust workspace (WASM target)
- built with Dioxus framework, compiled to WebAssembly
- one-time setup after clone: `cd web-ui && npm install` (installs tailwind/daisyui/fontawesome/material-icons and copies font files to `public/`)
- compile check: `dx check` (run from `web-ui/`)
- dev: `cargo make serve_dev` or `cd web-ui && dx serve`
- release: `cargo make build_ui_release` (output embedded in backend binary)
- CSS source is `web-ui/input.css`; compiled output is `web-ui/public/tw.css` (committed to git)
- regenerate CSS: `cargo make build_css`, then commit `public/tw.css`
- `public/fontawesome/` and `public/material-icons/` are gitignored — populated by `npm install`

### Backend
- Compile BE with `cargo make build_dev`
- Frontend release build must exist before compiling the backend (`cargo make build_ui_release` embeds the UI)
- Run locally with `cargo make run_local`. CWD dir is `.run`
- When deployed to target env it is run by systemd defined in PGKS

### Documentation
- docsify site in `docs/`
- README.md
- Release notes are here `docs/release_notes.md`

### Versioning
- version is centralized in Makefile.toml and it is used by `crates/server/build.rs`

### Deploy to local test env (RPI)
- there are two test devices rpi zero and rpi4 (rpi_host and rpi_target env variables in Makefile.toml) it requires switch remove/add comment...
- If FE files changed: `cargo make build_ui_release && cargo make copy_remote`
- Otherwise: `cargo make copy_remote`
