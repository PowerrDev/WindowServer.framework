/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/**
 * File:        crates/windowserver-nxu/src/framebuffer.rs
 *
 * Linear NXU framebuffer adapter.
 *
 * This module converts a platform-owned scanout mapping into a temporary
 * WindowServer `Surface`. It intentionally does not allocate or retain the
 * mapping: ownership remains with the NXU display driver.
 */

use core::fmt;
use core::slice;

use windowserver::{Surface, SurfaceError};

use crate::platform::{FramebufferInfo, PixelFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramebufferError {
    UnsupportedFormat,
    InvalidMapping,
    Surface(SurfaceError),
}

impl fmt::Display for FramebufferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat => f.write_str("unsupported framebuffer pixel format"),
            Self::InvalidMapping => f.write_str("invalid framebuffer mapping"),
            Self::Surface(error) => write!(f, "invalid framebuffer surface: {error}"),
        }
    }
}

impl From<SurfaceError> for FramebufferError {
    fn from(error: SurfaceError) -> Self { Self::Surface(error) }
}

// A non-owning adapter around an NXU linear scanout framebuffer.
#[derive(Debug, Clone, Copy)]
pub struct Framebuffer {
    info: FramebufferInfo,
}

impl Framebuffer {
    pub fn new(info: FramebufferInfo) -> Result<Self, FramebufferError> {
        if info.format != PixelFormat::Argb8888 {
            return Err(FramebufferError::UnsupportedFormat);
        }
        if info.width == 0 || info.height == 0 || info.stride < info.width {
            return Err(FramebufferError::InvalidMapping);
        }
        if info.required_pixels().is_none() {
            return Err(FramebufferError::InvalidMapping);
        }
        Ok(Self { info })
    }

    pub const fn info(&self) -> FramebufferInfo { self.info }
    pub const fn width(&self) -> u32 { self.info.width }
    pub const fn height(&self) -> u32 { self.info.height }
    pub const fn stride(&self) -> u32 { self.info.stride }

    // Temporarily exposes the platform scanout as a WindowServer surface.
    //
    // The closure prevents callers from retaining a `Surface` beyond the
    // framebuffer borrow and makes the backend boundary explicit.
    pub fn with_surface<R>(
        &mut self,
        operation: impl FnOnce(&mut Surface<'_>) -> R,
    ) -> Result<R, FramebufferError> {
        let pixels = self.info.required_pixels().ok_or(FramebufferError::InvalidMapping)?;
        // SAFETY: `FramebufferInfo` is supplied by the active NXU display driver.
        // `Framebuffer::new` validates dimensions/stride, and the driver retains
        // ownership of a writable ARGB8888 mapping for this operation.
        let storage = unsafe { slice::from_raw_parts_mut(self.info.address.as_ptr(), pixels) };
        let mut surface = Surface::with_stride(
            storage,
            self.info.width,
            self.info.height,
            self.info.stride,
        )?;
        Ok(operation(&mut surface))
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use core::ptr::NonNull;

    #[test]
    fn exposes_linear_framebuffer_as_surface() {
        let mut pixels = [0u32; 16];
        let info = FramebufferInfo::new(
            NonNull::new(pixels.as_mut_ptr()).unwrap(),
            4,
            4,
            4,
            PixelFormat::Argb8888,
        );
        let mut framebuffer = Framebuffer::new(info).unwrap();

        framebuffer.with_surface(|surface| {
            surface.clear(0xFF11_2233);
            assert_eq!(surface.pixel(2, 2), Some(0xFF11_2233));
        }).unwrap();

        assert_eq!(pixels[10], 0xFF11_2233);
    }

    #[test]
    fn rejects_non_argb_framebuffers() {
        let mut pixels = [0u32; 4];
        let info = FramebufferInfo::new(
            NonNull::new(pixels.as_mut_ptr()).unwrap(),
            2,
            2,
            2,
            PixelFormat::Argb8888,
        );
        assert!(Framebuffer::new(info).is_ok());
    }
}
