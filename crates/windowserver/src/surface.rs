/*!
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/*!
 * File:        crates/windowserver/src/surface.rs
 *
 * Pixel surface primitives. Allocation and presentation are intentionally left
 * to the framebuffer backend; this type only describes owned or borrowed pixels.
 */

use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceError {
    InvalidDimensions,
    BufferTooSmall,
}

impl fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions => f.write_str("invalid surface dimensions"),
            Self::BufferTooSmall => f.write_str("pixel buffer is too small"),
        }
    }
}

/// A mutable ARGB8888 pixel surface with an explicit stride in pixels.
pub struct Surface<'a> {
    pixels: &'a mut [u32],
    width: u32,
    height: u32,
    stride: u32,
}

impl<'a> Surface<'a> {
    pub fn new(pixels: &'a mut [u32], width: u32, height: u32) -> Result<Self, SurfaceError> {
        Self::with_stride(pixels, width, height, width)
    }

    pub fn with_stride(
        pixels: &'a mut [u32], width: u32, height: u32, stride: u32,
    ) -> Result<Self, SurfaceError> {
        if width == 0 || height == 0 || stride < width {
            return Err(SurfaceError::InvalidDimensions);
        }
        let required = (stride as usize)
            .checked_mul(height as usize)
            .ok_or(SurfaceError::InvalidDimensions)?;
        if pixels.len() < required { return Err(SurfaceError::BufferTooSmall); }
        Ok(Self { pixels, width, height, stride })
    }

    pub const fn width(&self) -> u32 { self.width }
    pub const fn height(&self) -> u32 { self.height }
    pub const fn stride(&self) -> u32 { self.stride }

    pub fn pixels(&self) -> &[u32] { self.pixels }
    pub fn pixels_mut(&mut self) -> &mut [u32] { self.pixels }

    pub fn clear(&mut self, pixel: u32) {
        for y in 0..self.height as usize {
            let row = y * self.stride as usize;
            self.pixels[row..row + self.width as usize].fill(pixel);
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<u32> {
        if x >= self.width || y >= self.height { return None; }
        Some(self.pixels[y as usize * self.stride as usize + x as usize])
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, pixel: u32) -> bool {
        if x >= self.width || y >= self.height { return false; }
        let index = y as usize * self.stride as usize + x as usize;
        self.pixels[index] = pixel;
        true
    }

    pub fn fill_rect(&mut self, x: u32, y: u32, width: u32, height: u32, pixel: u32) {
        let right = x.saturating_add(width).min(self.width);
        let bottom = y.saturating_add(height).min(self.height);
        if x >= right || y >= bottom { return; }
        for row in y..bottom {
            let start = row as usize * self.stride as usize + x as usize;
            let end = start + (right - x) as usize;
            self.pixels[start..end].fill(pixel);
        }
    }
}
