/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */

/**
 * File:        crates/windowserver-nxu/src/runtime.rs
 *
 * Minimal NXU runtime support for the WindowServer static library.
 *
 * WindowServer.framework is linked into the NXU kernel and therefore does not
 * own a userspace allocator or panic runtime. Allocation is delegated to the
 * NXU kernel heap.
 */

use core::{
    alloc::{GlobalAlloc, Layout},
    ffi::c_void,
};

const NXU_HEAP_ALIGNMENT: usize = 16;

struct NXUAllocator;

unsafe impl GlobalAlloc for NXUAllocator {
    unsafe fn alloc(
        &self,
        layout: Layout,
    ) -> *mut u8 {
        if layout.size() == 0 || layout.align() > NXU_HEAP_ALIGNMENT {
            return core::ptr::null_mut();
        }

        unsafe {
            kmalloc(layout.size()).cast()
        }
    }

    unsafe fn dealloc(
        &self,
        ptr: *mut u8,
        _layout: Layout,
    ) {
        if !ptr.is_null() {
            unsafe {
                let _ = kfree(ptr.cast());
            }
        }
    }
}

#[global_allocator]
static NXU_ALLOCATOR: NXUAllocator = NXUAllocator;

unsafe extern "C" {
    fn kmalloc(size: usize) -> *mut c_void;

    fn kfree(address: *mut c_void) -> bool;
}

#[panic_handler]
fn windowserver_panic(
    _info: &core::panic::PanicInfo,
) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
