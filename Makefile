.PHONY: clean check nxu nxu-i386

check:
	cargo check --workspace

clean:
	cargo clean

NXU_TARGET ?= aarch64-unknown-none-softfloat
NXU_MANIFEST := crates/windowserver-nxu/Cargo.toml
NXU_ARCHIVE := target/$(NXU_TARGET)/release/libwindowserver_nxu.rlib
BUILD ?= BUILD

NXU_SERVICE_MANIFEST := nxu/Cargo.toml
NXU_SERVICE_ARCHIVE := nxu/target/$(NXU_TARGET)/release/libwindowserver_nxu.a

nxu:
	cargo build \
		--manifest-path $(NXU_MANIFEST) \
		--release \
		--target $(NXU_TARGET)

	mkdir -p $(BUILD)

	cp $(NXU_ARCHIVE) $(BUILD)/libWindowServer.rlib

	cargo build \
		--manifest-path $(NXU_SERVICE_MANIFEST) \
		--release \
		--target $(NXU_TARGET)

	cp $(NXU_SERVICE_ARCHIVE) $(BUILD)/libWindowServerService.a

# i686-nxu-none.json is a custom bare-metal x86-32 target (no built-in rustc
# triple covers freestanding i686): soft-float, no SSE/MMX, matching this
# repo's arm64 target being softfloat for the same reason (the NXU i386 port
# does not save FPU/SSE context across a context switch either). A .json
# target has no prebuilt std, so building it needs nightly plus -Z build-std;
# NXU_TARGET's aarch64-unknown-none-softfloat is a real rustc triple with a
# prebuilt std, so the plain `nxu` target above is deliberately left alone.
NXU_I386_TARGET_SPEC := i686-nxu-none.json
NXU_I386_TARGET := i686-nxu-none
NXU_I386_ARCHIVE := target/$(NXU_I386_TARGET)/release/libwindowserver_nxu.rlib
NXU_I386_SERVICE_ARCHIVE := nxu/target/$(NXU_I386_TARGET)/release/libwindowserver_nxu.a
CARGO_BUILD_STD_FLAGS := -Z build-std=core,alloc,compiler_builtins -Z build-std-features=compiler-builtins-mem -Z json-target-spec

nxu-i386:
	cargo +nightly build \
		--manifest-path $(NXU_MANIFEST) \
		--release \
		$(CARGO_BUILD_STD_FLAGS) \
		--target $(NXU_I386_TARGET_SPEC)

	mkdir -p $(BUILD)

	cp $(NXU_I386_ARCHIVE) $(BUILD)/libWindowServer.rlib

	cargo +nightly build \
		--manifest-path $(NXU_SERVICE_MANIFEST) \
		--release \
		$(CARGO_BUILD_STD_FLAGS) \
		--target $(NXU_I386_TARGET_SPEC)

	cp $(NXU_I386_SERVICE_ARCHIVE) $(BUILD)/libWindowServerService.a


