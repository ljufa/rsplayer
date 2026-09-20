# F-Droid

`io.github.ljufa.rsplayer.yml` is a DRAFT metadata file for https://gitlab.com/fdroid/fdroiddata
(`metadata/io.github.ljufa.rsplayer.yml`). Listing texts and the icon live in
`fastlane/metadata/android/en-US` in this repo, F-Droid reads them from there.

Untested, known open points before opening the merge request:
- F-Droid's buildserver forbids network access in `build:` and downloading toolchains with
  `curl | sh`; cargo/npm dependencies need pre-fetching in `prebuild:` (with `cargo vendor` / `--locked`)
  and rustup is normally installed through `sudo:` steps. Expect reviewers to ask for changes.
- `build_css` uses `npx tailwindcss` (npm download).
- Android versionCode is computed by the tauri CLI: major*1000000 + minor*1000 + patch.
- Verify locally: `fdroid readmeta && fdroid rewritemeta io.github.ljufa.rsplayer && fdroid build -v -l io.github.ljufa.rsplayer`.
- Screenshots: add PNGs under `fastlane/metadata/android/en-US/images/phoneScreenshots/`.
