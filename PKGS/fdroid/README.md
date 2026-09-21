# F-Droid

`de.rsplayer.app.yml` is a reference copy of the app's metadata in
https://gitlab.com/fdroid/fdroiddata (`metadata/de.rsplayer.app.yml`). The copy in fdroiddata is the
one F-Droid uses; it differs only in `commit:`, which there is the full hash of the release tag
(a file cannot contain the hash of its own commit), while this copy names the tag.

The store listing (title, descriptions, icon, screenshots, changelogs) is not in the recipe:
F-Droid reads it from `fastlane/metadata/android/en-US` in this repository.

## Reproducible builds

The recipe sets `binary:` (our signed per-ABI APKs on the GitHub release) and
`AllowedAPKSigningKeys` (SHA-256 of the release certificate), so F-Droid builds the tag from source,
checks that the result matches our APK and publishes our file with our signature. See
"Reproducible builds (F-Droid)" in `docs/build.md`. The build steps use the same pinned tool versions
as `crates/desktop/android-build-env.sh`; change them together.

## Releasing an update

1. Bump the version everywhere (see "Releasing the Android app" in `docs/build.md`), commit, tag `X.Y.Z`.
2. The "Full release" workflow uploads the signed APKs to a draft release. **Publish the draft**:
   F-Droid cannot download from a draft.
3. F-Droid's `checkupdates` finds the tag, copies the last build entries with the new version codes and
   commit, and builds them. If a pinned tool changed, send a merge request to fdroiddata that updates
   the recipe.

Never move or delete a published tag: the recipe pins its commit.

## Checking a recipe locally

```
fdroid readmeta && fdroid rewritemeta de.rsplayer.app && fdroid lint de.rsplayer.app
```

The full build takes about 50 minutes on F-Droid's CI; there is no point running it locally.
