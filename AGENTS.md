### Don't ever do
- git write commands: commit, push, merge, rebase ...

### Frontend
- frontend location is `rsplayer_web_ui` and it is not member of main rust workspace `rsplayer`
- frontend is rust app with seed framework and it is compiled to web assembly
- Compile FE with `cargo make build_ui_dev` from this dir or `cargo make build_dev` from `rsplayer_web_ui` dir

### Backend
- Compile BE with `cargo make build_dev`
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
- `cargo make build_ui_release` && `cargo make copy_remote`