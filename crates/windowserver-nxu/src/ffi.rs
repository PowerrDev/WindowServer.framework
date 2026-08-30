/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/**
 * File:        crates/windowserver-nxu/src/ffi.rs
 *
 * C ABI bootstrap for the NXU WindowServer backend.
 *
 * The NXU kernel owns display discovery and framebuffer presentation. This
 * module only adapts the active linear scanout mapping and renders the first
 * WindowServer test scene into it.
 */

use core::ptr::NonNull;

use windowserver::{Rect, WindowServer};

use crate::{Framebuffer, FramebufferInfo, PixelFormat};

/// Bootstraps the WindowServer against an NXU-owned ARGB8888 framebuffer.
///
/// # Safety
///
/// `framebuffer` must point to at least `stride * height` writable `u32`
/// pixels for the duration of this call. The mapping remains owned by NXU.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn windowserver_nxu_bootstrap(
    framebuffer: *mut u32,
    width: u32,
    height: u32,
    stride: u32,
) -> bool {
    let Some(address) = NonNull::new(framebuffer) else {
        return false;
    };

    let info = FramebufferInfo::new(
        address,
        width,
        height,
        stride,
        PixelFormat::Argb8888,
    );

    let Ok(mut framebuffer) = Framebuffer::new(info) else {
        return false;
    };

    let mut server = WindowServer::new(0xFF1A_1A_1A);

    let first = match server.create_window(Rect::new(80, 80, 420, 280)) {
        Ok(id) => id,
        Err(_) => return false,
    };
    if !server.fill_window(first, 0xFF2D_6C_BA) {
        return false;
    }

    let second = match server.create_window(Rect::new(260, 190, 420, 280)) {
        Ok(id) => id,
        Err(_) => return false,
    };
    if !server.fill_window(second, 0xFFC4_3D_5A) {
        return false;
    }

    match framebuffer.with_surface(|surface| {
        surface.clear(0xFF1A_1A_1A);
        server.present(surface)
    }) {
        Ok(Ok(_)) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::windowserver_nxu_bootstrap;

    #[test]
    fn bootstrap_composes_test_scene() {
        let mut pixels = [0u32; 800 * 600];
        assert!(unsafe {
            windowserver_nxu_bootstrap(pixels.as_mut_ptr(), 800, 600, 800)
        });
        assert_eq!(pixels[0], 0xFF1A_1A_1A);
        assert_eq!(pixels[100 * 800 + 100], 0xFF2D_6C_BA);
        assert_eq!(pixels[200 * 800 + 300], 0xFFC4_3D_5A);
    }
}
