RELEASE=rsplayer
TARGET=aarch64-unknown-linux-gnu
IMAGE=ljufa/rpi4-build-ak44:aarch64_2
RPI_HOST=192.168.5.60
RUST_BACKTRACE=full
OUT=release


.ONESHELL:
build_librespot:
	cd ..
	git clone git@github.com:librespot-org/librespot.git
	git checkout master
	cp rsplayer/Cross.toml librespot/
	cd librespot
	cross build --target $(TARGET) --release --no-default-features --features alsa-backend
	cp -f target/$(TARGET)/$(OUT)/librespot ../rsplayer/PKGS/rsplayer/urs/local/bin

release:
	cargo fmt
	cross build --target $(TARGET) --release 
	cp target/$(TARGET)/$(OUT)/$(RELEASE) PKGS/rsplayer/usr/local/bin

debug:
	cargo fmt
	cross build --target $(TARGET)

copytorpi: $(OUT)
	rsync -avvP --rsync-path="sudo rsync" target/$(TARGET)/$(OUT)/$(RELEASE) pi@$(RPI_HOST):/usr/local/bin

.ONESHELL:
build_ui:
	cd rsplayer_web_ui
	cargo make build_release
	rsync -av --delete ./pkg ./public/ ../PKGS/rsplayer/opt/rsplayer/ui
	

create_deb: release build_ui
	dpkg-deb -b -Zxz --root-owner-group PKGS/rsplayer
	# rsync -avvP PKGS/*.deb pi@$(RPI_HOST):~/


### Local development @ linux x86_64
.ONESHELL:
build_ui_local:
	cd rsplayer_web_ui
	cargo make build_dev
	rsync -av --delete ./pkg ./public/ ../.run/ui

run_local:
	cargo clippy
	pkill librespot || true
	pkill rsplayer || true
	RUST_BACKTRACE=full RUST_LOG=info,rsplayer=debug,rspotify=info,librespot=debug cargo run

.ONESHELL:
build_librespot_local:
	cd ..
	# git clone git@github.com:librespot-org/librespot.git
	# git checkout master
	cd librespot
	cargo build --release --no-default-features --features alsa-backend
	cp -f target/release/librespot ../rsplayer/.run/

	


	