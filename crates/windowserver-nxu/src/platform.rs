/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/**
 * File:        crates/windowserver-nxu/src/platform.rs
 *
 * NXU display platform abstraction.
 *
 * The platform layer owns discovery of the real scanout framebuffer. The
 * WindowServer backend deliberately only needs a linear ARGB8888 mapping and
 * therefore remains independent of VirtIO-GPU or any future NXU display driver.
 */

use core::ptr::NonNull;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Argb8888,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramebufferInfo {
    pub address: NonNull<u32>,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
}

impl FramebufferInfo {
    pub const fn new(
        address: NonNull<u32>,
        width: u32,
        height: u32,
        stride: u32,
        format: PixelFormat,
    ) -> Self {
        Self { address, width, height, stride, format }
    }

    pub fn required_pixels(self) -> Option<usize> {
        (self.stride as usize).checked_mul(self.height as usize)
    }
}

/// NXU display implementations expose their current linear scanout through
/// this trait. `windowserver-nxu` does not own the driver or presentation clock.
pub trait DisplayPlatform {
    fn framebuffer(&mut self) -> Option<FramebufferInfo>;
    fn present(&mut self);
}
