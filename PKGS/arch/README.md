# Arch Linux packages

Two pacman packages, both repackaging the prebuilt release binaries:

| Package | Contents | Architectures |
|:---|:---|:---|
| `rsplayer-bin` | headless server, systemd unit, polkit rule, `/opt/rsplayer` defaults | x86_64, aarch64, armv7h, armv6h, riscv64 |
| `rsplayer-desktop-bin` | desktop app, pkexec mount helper, polkit action, icons | x86_64, aarch64 |

Each PKGBUILD's source is the release `.tgz` for that arch
(`rsplayer_<ver>_<suffix>.tgz`, `rsplayer-desktop_<ver>_<suffix>.tgz`, built
by `package_arch_release` / `bundle_desktop_release*` in `Makefile.toml`).
One PKGBUILD serves both distribution routes, so users can switch between them:

- **Release asset**: `rsplayer-bin-<ver>-1-<arch>.pkg.tar.zst`, installed by
  `install.sh` / `install_desktop.sh` with `pacman -U`.
- **AUR**: <https://aur.archlinux.org/packages/rsplayer-bin> and
  `rsplayer-desktop-bin`, installed with `yay -S rsplayer-bin` etc.

## Files

```
rsplayer-bin/PKGBUILD.in           template: @VERSION@, @LICENSE_SHA256@, @ARCH_SOURCES@
rsplayer-bin/rsplayer-bin.install  service user, enable/restart on install/upgrade
rsplayer-desktop-bin/PKGBUILD.in   template (no .install: pacman hooks refresh icon/desktop caches)
desktop/rsplayer-desktop.desktop   .desktop file put into the desktop tgz
render-pkgbuild.sh                 template -> PKGBUILD with per-arch source URLs + sha256sums
build-pkgs.sh                      render + makepkg for every arch (run on Arch)
```

`render-pkgbuild.sh` computes the checksums from local copies of the release
tgz files and leaves out any arch whose tgz is missing. The tgz files carry no
license file, so `LICENSE` is a second source, fetched from the release tag
(its checksum comes from the checkout, which must be at that tag).

`build-pkgs.sh` packages foreign arches on an x86_64 host by overriding
`CARCH` in a copy of `makepkg.conf` (with `--nodeps`, and `!strip` in the
PKGBUILD). makepkg finds the tgz next to the PKGBUILD, so it builds before
the release is published.

## Building

- CI: `build_arch_pkgs` job in `.github/workflows/cd.yml` (archlinux container),
  attached to the draft release as `pkg-arch`.
- Locally: `cargo make package_arch_pkgs` (Docker). Takes the tgz files from
  `target/cross/*/pkg/`, or from `ARCH_ASSETS=<dir>`; output in `target/arch-pkg/`,
  rendered PKGBUILDs in `target/arch-pkg/aur/`.

Install a local build with `sudo pacman -U rsplayer-bin-<ver>-1-x86_64.pkg.tar.zst`.

## Upgrading from the old tarball install

Earlier installers extracted the tgz to `/`. The install scripts take such an
install over: they remove `/etc/systemd/system/rsplayer.service` and
`/etc/polkit-1/rules.d/99-rsplayer.rules` (the package ships them under
`/usr/lib/systemd/system` and `/usr/share/polkit-1/rules.d`) and pass
`--overwrite` for each file of the package that is on disk without an owner.
A changed `/opt/rsplayer/env` is kept; pacman saves the new default as
`env.pacnew`.

## AUR publishing

`.github/workflows/aur.yml` runs when a release is promoted to a full release
(or by dispatch with a tag). It downloads the release tgz files, renders both
PKGBUILDs, generates `.SRCINFO` and pushes to
`ssh://aur@aur.archlinux.org/<pkgname>.git`. The first push creates the package.

One-time setup:

1. Create an account on <https://aur.archlinux.org>.
2. Generate a key for CI: `ssh-keygen -t ed25519 -f aur -C rsplayer-ci`, and
   add `aur.pub` under **My Account → SSH Public Key**.
3. Add the private key as the repository secret `AUR_SSH_PRIVATE_KEY`.
4. Dispatch **Publish AUR packages** with the current release tag to create
   both packages.

## Checking a package

`namcap` reports one error that is kept on purpose: the mount helper lives in
`/usr/libexec` (Arch prefers `/usr/lib/<pkg>`), because the path is compiled
into the shared binary (`MOUNT_HELPER_PATH` in
`crates/server/src/mount_service_linux.rs`) and named in the polkit policy,
the same for deb, rpm and Arch.

```bash
namcap rsplayer-bin-<ver>-1-x86_64.pkg.tar.zst   # lint
pacman -Qlp rsplayer-bin-<ver>-1-x86_64.pkg.tar.zst
pacman -Qkk rsplayer-bin                            # after install
```
