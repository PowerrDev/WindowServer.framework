/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */

/**
 * File:        crates/windowserver-nxu/src/runtime.rs
 *
 * Minimal NXU runtime support for the WindowServer static library.
 *
 * windowserver-nxu links either straight into the NXU kernel (the legacy
 * in-kernel path) or into a standalone userland WindowServer process (see
 * frameworks/WindowServer.framework/windowserver_service.c in the nxu
 * repo); either way it does not own its own allocator, so allocation is
 * delegated to whichever host provides `kmalloc`/`kfree` at link time --
 * the kernel heap for the former, a small userland arena allocator for the
 * latter.
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

        unsafe { kmalloc(layout.size()).cast() }
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
