#![no_std]

extern crate alloc;

/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */

/**
 * File:        nxu/src/lib.rs
 *
 * NXU freestanding runtime boundary for WindowServer.framework.
 */

use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;

struct NxuAllocator;

unsafe extern "C" {
    fn kmalloc(size: usize) -> *mut core::ffi::c_void;
    fn kfree(address: *mut core::ffi::c_void) -> bool;
}

unsafe impl GlobalAlloc for NxuAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return layout.align() as *mut u8;
        }
        if layout.align() > core::mem::align_of::<usize>() {
            return core::ptr::null_mut();
        }
        unsafe { kmalloc(layout.size()) as *mut u8 }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        if !ptr.is_null() {
            let _ = unsafe { kfree(ptr.cast()) };
        }
    }
}

#[global_allocator]
static NXU_ALLOCATOR: NxuAllocator = NxuAllocator;

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop { core::hint::spin_loop(); }
}

#[unsafe(no_mangle)]
pub extern "C" fn windowserver_nxu_bootstrap(
    framebuffer: *mut u32,
    width: u32,
    height: u32,
    stride: u32,
) -> bool {
    if framebuffer.is_null() || width == 0 || height == 0 || stride < width {
        return false;
    }

    unsafe {
        windowserver_nxu::ffi::bootstrap(
            framebuffer,
            width,
            height,
            stride,
        )
    }
}
