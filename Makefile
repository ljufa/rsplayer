RELEASE=dplay
TARGET=aarch64-unknown-linux-gnu
IMAGE=ljufa/rpi4-build-ak44:aarch64_2
RPI_HOST=192.168.44.26
RUST_BACKTRACE=full
OUT=release


.ONESHELL:
build_librespot:
	cd ..
	git clone git@github.com:librespot-org/librespot.git
	cp dplayer/Cross.toml librespot/
	cd librespot
	cross build --target $(TARGET) --release --no-default-features --features alsa-backend
	rsync --rsync-path="sudo rsync" target/$(TARGET)/$(OUT)/librespot ubuntu@$(RPI_HOST):/home/ubuntu
	
check:
	STRUM_DEBUG=1 cross check --target $(TARGET) --package dplay:0.1.0 --bin dplay

clippy:
	cross clippy --target $(TARGET) --package dplay:0.1.0 --bin dplay

test:
	cross test --target $(TARGET) --package dplay:0.1.0 --bin dplay -- --nocapture

release:
	cargo fmt
	cross --offline --frozen build --target $(TARGET) --release

debug:
	cargo fmt
	cross build --target $(TARGET)

copytorpi: $(OUT)
	rsync -avvP --rsync-path="sudo rsync" target/$(TARGET)/$(OUT)/$(RELEASE) ubuntu@$(RPI_HOST):/home/ubuntu

copy_config:
	rsync -avvP --rsync-path="sudo rsync" rpi_setup/etc/ ubuntu@$(RPI_HOST):/etc
	rsync -avvP rpi_setup/.dplay/ ubuntu@$(RPI_HOST):~/.dplay/

clean:
	cargo clean
	
