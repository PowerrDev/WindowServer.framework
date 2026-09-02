.PHONY: check nxu

check:
	cargo check --workspace

NXU_TARGET ?= aarch64-unknown-none-softfloat
NXU_MANIFEST := crates/windowserver-nxu/Cargo.toml
NXU_ARCHIVE := target/$(NXU_TARGET)/release/libwindowserver_nxu.a
BUILD ?= BUILD

nxu:
	cargo build \
		--manifest-path $(NXU_MANIFEST) \
		--release \
		--target $(NXU_TARGET)

	mkdir -p $(BUILD)

	cp $(NXU_ARCHIVE) $(BUILD)/libWindowServer.a


