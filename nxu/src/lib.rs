#![no_std]

/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */

/**
 * File:        nxu/src/lib.rs
 *
 * Standalone-staticlib wrapper around windowserver-nxu: supplies the panic
 * handler a freestanding userland process needs (windowserver-nxu already
 * supplies the `#[global_allocator]`, delegating to the same `kmalloc`/
 * `kfree` externs this build links against -- see
 * crates/windowserver-nxu/src/runtime.rs; the legacy in-kernel path gets a
 * panic handler from whatever links windowserver-nxu's plain rlib in
 * instead, so this crate is only built for the standalone service). Simply
 * depending on windowserver-nxu is enough for its WS_* functions
 * (WSPrivate.h, exported with explicit `export_name`s) to appear in this
 * staticlib's output archive -- no re-export needed.
 */

use core::panic::PanicInfo;

#[allow(unused_imports)]
use windowserver_nxu as _;

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop { core::hint::spin_loop(); }
}
