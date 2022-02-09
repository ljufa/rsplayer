RELEASE=dplay
TARGET=aarch64-unknown-linux-gnu
IMAGE=ljufa/rpi4-build-ak44:aarch64_2
RPI_HOST=192.168.5.59
RUST_BACKTRACE=full
OUT=release


.ONESHELL:
build_librespot:
	cd ..
	git clone git@github.com:librespot-org/librespot.git
	cp dplayer/Cross.toml librespot/
	cd librespot
	cross build --target $(TARGET) --release --no-default-features --features alsa-backend
	cp -f target/$(TARGET)/$(OUT)/librespot ../dplayer/rpi_setup/.dplay/librespot


test:
	cross test --target $(TARGET) --package dplay:0.1.0 --bin dplay

check:
	cross check --target $(TARGET) --package dplay:0.1.0 --bin dplay

release:
	cargo fmt
	cross build --target $(TARGET) --release

copytorpi: $(OUT)
	rsync -avvP --rsync-path="sudo rsync" target/$(TARGET)/$(OUT)/$(RELEASE) ubuntu@$(RPI_HOST):/home/ubuntu

copy_config:
	rsync -avvP --rsync-path="sudo rsync" rpi_setup/etc/ ubuntu@$(RPI_HOST):/etc
	rsync -avvP rpi_setup/.dplay/ ubuntu@$(RPI_HOST):~/.dplay/

clean:
	cargo clean
	
build_local:
	cargo build

test:
	cargo test

