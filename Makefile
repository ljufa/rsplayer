RELEASE=dplay
TARGET=aarch64-unknown-linux-gnu
IMAGE=ljufa/rpi4-build-ak44:aarch64_2
RPI_HOST=192.168.5.60
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

release:
	cargo fmt
	cross build --target $(TARGET) --release --features backend_mpd,backend_lms,hw_dac,hw_ir_control,hw_oled

debug:
	cargo fmt
	cross build --target $(TARGET) --features backend_mpd,backend_lms,hw_dac,hw_ir_control,hw_oled

copytorpi: $(OUT)
	rsync -avvP --rsync-path="sudo rsync" target/$(TARGET)/$(OUT)/$(RELEASE) pi@$(RPI_HOST):~

copy_config:
	rsync -avvP --rsync-path="sudo rsync" rpi_setup/etc/ pi@$(RPI_HOST):/etc
	rsync -avvP --rsync-path="sudo rsync" rpi_setup/config.txt pi@$(RPI_HOST):/boot/config.txt
	rsync -avvP rpi_setup/.dplay/ pi@$(RPI_HOST):~/.dplay/


