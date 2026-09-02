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

/// Serializable framebuffer metadata.
///
/// Unlike `FramebufferInfo`, this type does not contain a raw pointer. It is
/// therefore suitable for storage inside synchronized global runtime state.
///
/// The actual pointer is reconstructed only at the platform boundary where
/// framebuffer access occurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramebufferDescriptor {
    /// Linear scanout address supplied by NXU.
    pub address: usize,

    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
}

impl FramebufferDescriptor {
    pub const fn new(
        address: usize,
        width: u32,
        height: u32,
        stride: u32,
        format: PixelFormat,
    ) -> Self {
        Self {
            address,
            width,
            height,
            stride,
            format,
        }
    }

    /// Reconstruct framebuffer metadata for platform access.
    ///
    /// Returns `None` when the stored address is null.
    pub fn framebuffer_info(self) -> Option<FramebufferInfo> {
        let address = NonNull::new(self.address as *mut u32)?;

        Some(FramebufferInfo {
            address,
            width: self.width,
            height: self.height,
            stride: self.stride,
            format: self.format,
        })
    }
}

/// Runtime framebuffer mapping.
///
/// This represents an actual linear ARGB8888 framebuffer and deliberately
/// retains its non-null pointer type. Pointer-bearing framebuffer state should
/// remain at the platform boundary rather than being stored directly in global
/// synchronized runtime state.
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
        Self {
            address,
            width,
            height,
            stride,
            format,
        }
    }

    /// Returns a pointer-free descriptor suitable for persistent runtime state.
    pub fn descriptor(self) -> FramebufferDescriptor {
        FramebufferDescriptor {
            address: self.address.as_ptr() as usize,
            width: self.width,
            height: self.height,
            stride: self.stride,
            format: self.format,
        }
    }

    pub fn required_pixels(self) -> Option<usize> {
        (self.stride as usize).checked_mul(self.height as usize)
    }
}

impl From<FramebufferInfo> for FramebufferDescriptor {
    fn from(info: FramebufferInfo) -> Self {
        info.descriptor()
    }
}

/// NXU display implementations expose their current linear scanout through
/// this trait. `windowserver-nxu` does not own the driver or presentation
/// clock.
pub trait DisplayPlatform {
    fn framebuffer(&mut self) -> Option<FramebufferInfo>;
    fn present(&mut self);
}
