TARGET=aarch64-unknown-linux-gnu
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

.ONESHELL:
build_librespot_local:
	cd ..
	# git clone git@github.com:librespot-org/librespot.git
	# git checkout master
	cd librespot
	cargo build --release --no-default-features --features alsa-backend
	cp -f target/release/librespot ../rsplayer/.run/

	


	