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
	cp rsplayer-backend/Cross.toml librespot/
	cd librespot
	cross build --target $(TARGET) --release --no-default-features --features alsa-backend
	cp -f target/$(TARGET)/$(OUT)/librespot ../rsplayer-backend/PKGS/rsplayer/urs/local/bin

release:
	cargo fmt
	cross build --target $(TARGET) --release 
	cp target/$(TARGET)/$(OUT)/$(RELEASE) PKGS/rsplayer/usr/local/bin

debug:
	cargo fmt
	cross build --target $(TARGET)

copytorpi: $(OUT)
	rsync -avvP --rsync-path="sudo rsync" target/$(TARGET)/$(OUT)/$(RELEASE) pi@$(RPI_HOST):~

# copy_config:
# 	rsync -avvP --rsync-path="sudo rsync" rpi_setup/etc/ pi@$(RPI_HOST):/etc
# 	rsync -avvP --rsync-path="sudo rsync" rpi_setup/config.txt pi@$(RPI_HOST):/boot/config.txt
# 	rsync -avvP rpi_setup/.dplay/librespot pi@$(RPI_HOST):~/.dplay/

run_local:
	cargo clippy
	pkill librespot || true
	pkill rsplayer || true
	RUST_BACKTRACE=full RUST_LOG=info,rsplayer=debug,rspotify=info,librespot=debug cargo run

.ONESHELL:
build_ui:
	cd ../rsplayer-ui
	cargo make build_release
	

.ONESHELL:
build_ui_dev:
	cd ../rsplayer-ui
	cargo make build_dev
	rsync -av --delete ./pkg ./public/ ../rsplayer-backend/.run/ui
	

create_deb: release build_ui
	rsync -av --delete ../rsplayer-ui/pkg ../rsplayer-ui/public/ PKGS/rsplayer/opt/rsplayer/ui
	dpkg-deb -b -Zxz --root-owner-group PKGS/rsplayer
	rsync -avvP PKGS/*.deb pi@$(RPI_HOST):~/

	