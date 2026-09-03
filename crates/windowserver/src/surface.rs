/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */

/**
 * File:        crates/windowserver/src/surface.rs
 *
 * Pixel surface primitives. Allocation and presentation are intentionally left
 * to the framebuffer backend; this type only describes owned or borrowed pixels.
 */

use core::fmt;

use crate::geometry::{
    Point,
    Rect,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceError {
    InvalidDimensions,
    BufferTooSmall,
}

impl fmt::Display for SurfaceError {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            Self::InvalidDimensions => {
                f.write_str("invalid surface dimensions")
            }

            Self::BufferTooSmall => {
                f.write_str("pixel buffer is too small")
            }
        }
    }
}

// A mutable ARGB8888 pixel surface with an explicit stride in pixels.
pub struct Surface<'a> {
    pixels: &'a mut [u32],

    width: u32,

    height: u32,

    stride: u32,
}

impl<'a> Surface<'a> {
    pub fn new(
        pixels: &'a mut [u32],
        width: u32,
        height: u32,
    ) -> Result<Self, SurfaceError> {
        Self::with_stride(
            pixels,
            width,
            height,
            width,
        )
    }

    pub fn with_stride(
        pixels: &'a mut [u32],
        width: u32,
        height: u32,
        stride: u32,
    ) -> Result<Self, SurfaceError> {
        if width == 0
            || height == 0
            || stride < width
        {
            return Err(SurfaceError::InvalidDimensions);
        }

        let required = (stride as usize)
            .checked_mul(height as usize)
            .ok_or(SurfaceError::InvalidDimensions)?;

        if pixels.len() < required {
            return Err(SurfaceError::BufferTooSmall);
        }

        Ok(Self {
            pixels,
            width,
            height,
            stride,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn stride(&self) -> u32 {
        self.stride
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(
            0,
            0,
            self.width,
            self.height,
        )
    }

    pub fn pixels(&self) -> &[u32] {
        self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [u32] {
        self.pixels
    }

    pub fn clear(
        &mut self,
        pixel: u32,
    ) {
        for y in 0..self.height as usize {
            let row =
                y * self.stride as usize;

            self.pixels[
                row..row + self.width as usize
            ].fill(pixel);
        }
    }

    pub fn pixel(
        &self,
        x: u32,
        y: u32,
    ) -> Option<u32> {
        if x >= self.width
            || y >= self.height
        {
            return None;
        }

        Some(
            self.pixels[
                y as usize * self.stride as usize
                    + x as usize
            ]
        )
    }

    pub fn set_pixel(
        &mut self,
        x: u32,
        y: u32,
        pixel: u32,
    ) -> bool {
        if x >= self.width
            || y >= self.height
        {
            return false;
        }

        let index =
            y as usize * self.stride as usize
                + x as usize;

        self.pixels[index] = pixel;

        true
    }

    pub fn set_pixel_at(
        &mut self,
        point: Point,
        pixel: u32,
    ) -> bool {
        if point.x < 0
            || point.y < 0
        {
            return false;
        }

        self.set_pixel(
            point.x as u32,
            point.y as u32,
            pixel,
        )
    }

    pub fn fill_rect(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        pixel: u32,
    ) {
        let right = x
            .saturating_add(width)
            .min(self.width);

        let bottom = y
            .saturating_add(height)
            .min(self.height);

        if x >= right
            || y >= bottom
        {
            return;
        }

        for row in y..bottom {
            let start =
                row as usize * self.stride as usize
                    + x as usize;

            let end =
                start + (right - x) as usize;

            self.pixels[start..end].fill(pixel);
        }
    }

    // Fill a geometry rectangle, automatically clipping it
    // against the surface bounds.
    pub fn fill_rect_rect(
        &mut self,
        rect: Rect,
        pixel: u32,
    ) {
        let Some(rect) =
            rect.intersection(self.bounds())
        else {
            return;
        };

        self.fill_rect(
            rect.x() as u32,
            rect.y() as u32,
            rect.width(),
            rect.height(),
            pixel,
        );
    }

    pub fn draw_hline(
        &mut self,
        x: i32,
        y: i32,
        width: u32,
        pixel: u32,
    ) {
        if y < 0 {
            return;
        }

        self.fill_rect_rect(
            Rect::new(
                x,
                y,
                width,
                1,
            ),
            pixel,
        );
    }

    pub fn draw_vline(
        &mut self,
        x: i32,
        y: i32,
        height: u32,
        pixel: u32,
    ) {
        if x < 0 {
            return;
        }

        self.fill_rect_rect(
            Rect::new(
                x,
                y,
                1,
                height,
            ),
            pixel,
        );
    }

    // Draw a one-pixel rectangle outline.
    pub fn stroke_rect(
        &mut self,
        rect: Rect,
        pixel: u32,
    ) {
        if rect.is_empty() {
            return;
        }

        self.draw_hline(
            rect.x(),
            rect.y(),
            rect.width(),
            pixel,
        );

        if rect.height() > 1 {
            self.draw_hline(
                rect.x(),
                rect.bottom() as i32 - 1,
                rect.width(),
                pixel,
            );
        }

        self.draw_vline(
            rect.x(),
            rect.y(),
            rect.height(),
            pixel,
        );

        if rect.width() > 1 {
            self.draw_vline(
                rect.right() as i32 - 1,
                rect.y(),
                rect.height(),
                pixel,
            );
        }
    }

    // Draw a 1-bit monochrome bitmap.
    //
    // Each bit represents one pixel:
    //
    // - 1 = foreground
    // - 0 = transparent
    //
    // `bitmap_stride` is measured in bytes.
    pub fn draw_bitmap_1bpp(
        &mut self,
        position: Point,
        width: u32,
        height: u32,
        bitmap: &[u8],
        bitmap_stride: u32,
        foreground: u32,
    ) {
        if width == 0 || height == 0 {
            return;
        }

        if bitmap_stride == 0 {
            return;
        }

        let required =
            (bitmap_stride as usize)
                .checked_mul(height as usize);

        let Some(required) = required else {
            return;
        };

        if bitmap.len() < required {
            return;
        }

        /*
        * Iterate over the source bitmap.
        *
        * Clipping happens naturally through set_pixel_at(),
        * so glyphs may safely extend outside the framebuffer.
        */
        for source_y in 0..height {
            for source_x in 0..width {
                let byte_index =
                    source_y as usize * bitmap_stride as usize
                        + (source_x / 8) as usize;

                let bit_index = 7 - (source_x % 8);

                let byte = bitmap[byte_index];

                let visible =
                    (byte & (1 << bit_index)) != 0;

                if !visible {
                    continue;
                }

                let destination = Point::new(
                    position.x.saturating_add(source_x as i32),
                    position.y.saturating_add(source_y as i32),
                );

                self.set_pixel_at(
                    destination,
                    foreground,
                );
            }
        }
    }
}