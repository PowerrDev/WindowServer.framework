/*!
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/*!
 * File:        crates/windowserver/src/compositor.rs
 *
 * Minimal software compositor.
 *
 * This first implementation deliberately has no decorations, shadows, blur or
 * platform knowledge. It composes window surfaces back-to-front into a target
 * ARGB8888 surface and restricts work to the supplied damage rectangle.
 */

use crate::{geometry::Rect, surface::Surface, window::Window};

/// A window together with the pixels currently attached to it.
///
/// Window lifetime/state ownership remains with `server.rs`; the compositor only
/// borrows a snapshot of the layers it must present.
pub struct Layer<'a, 'pixels> {
    pub window: &'a Window,
    pub surface: &'a Surface<'pixels>,
}

impl<'a, 'pixels> Layer<'a, 'pixels> {
    pub const fn new(window: &'a Window, surface: &'a Surface<'pixels>) -> Self {
        Self { window, surface }
    }
}

/// Platform-independent software compositor.
pub struct Compositor {
    background: u32,
}

impl Compositor {
    pub const fn new(background: u32) -> Self { Self { background } }
    pub const fn background(&self) -> u32 { self.background }
    pub fn set_background(&mut self, background: u32) { self.background = background; }

    /// Compose `layers` in slice order: first is backmost, last is frontmost.
    pub fn compose<'a, 'pixels>(
        &self,
        framebuffer: &mut Surface<'_>,
        layers: &[Layer<'a, 'pixels>],
        damage: Rect,
    ) {
        let Some(clip) = damage.intersection(surface_rect(framebuffer)) else { return; };

        fill_rect(framebuffer, clip, self.background);

        for layer in layers {
            if !layer.window.flags().visible { continue; }
            self.compose_layer(framebuffer, layer, clip);
        }
    }

    fn compose_layer<'a, 'pixels>(
        &self,
        framebuffer: &mut Surface<'_>,
        layer: &Layer<'a, 'pixels>,
        clip: Rect,
    ) {
        let frame = layer.window.frame();
        let Some(area) = frame.intersection(clip) else { return; };

        let source = layer.surface;
        let copy_width = frame.width().min(source.width());
        let copy_height = frame.height().min(source.height());
        let drawable = Rect::new(frame.x(), frame.y(), copy_width, copy_height);
        let Some(area) = drawable.intersection(area) else { return; };

        let opaque = layer.window.flags().opaque;
        for y in area.y()..area.bottom() as i32 {
            for x in area.x()..area.right() as i32 {
                let sx = (x - frame.x()) as u32;
                let sy = (y - frame.y()) as u32;
                let dx = x as u32;
                let dy = y as u32;
                let Some(source_pixel) = source.pixel(sx, sy) else { continue; };

                if opaque || source_pixel >> 24 == 0xFF {
                    framebuffer.set_pixel(dx, dy, 0xFF00_0000 | (source_pixel & 0x00FF_FFFF));
                } else if source_pixel >> 24 != 0 {
                    let destination = framebuffer.pixel(dx, dy).unwrap_or(0);
                    framebuffer.set_pixel(dx, dy, blend(destination, source_pixel));
                }
            }
        }
    }
}

fn surface_rect(surface: &Surface<'_>) -> Rect {
    Rect::new(0, 0, surface.width(), surface.height())
}

fn fill_rect(surface: &mut Surface<'_>, rect: Rect, pixel: u32) {
    if rect.x() < 0 || rect.y() < 0 { return; }
    surface.fill_rect(rect.x() as u32, rect.y() as u32, rect.width(), rect.height(), pixel);
}

fn blend(destination: u32, source: u32) -> u32 {
    let alpha = source >> 24;
    if alpha == 0 { return destination; }
    if alpha == 255 { return 0xFF00_0000 | (source & 0x00FF_FFFF); }

    let inverse = 255 - alpha;
    let channel = |shift: u32| -> u32 {
        let src = (source >> shift) & 0xFF;
        let dst = (destination >> shift) & 0xFF;
        (src * alpha + dst * inverse + 127) / 255
    };

    0xFF00_0000 | (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{geometry::Rect, surface::Surface, window::Window};

    #[test]
    fn front_layer_overwrites_back_layer() {
        let mut framebuffer_pixels = [0u32; 64];
        let mut framebuffer = Surface::new(&mut framebuffer_pixels, 8, 8).unwrap();
        let mut back_pixels = [0xFFFF_0000u32; 16];
        let back_surface = Surface::new(&mut back_pixels, 4, 4).unwrap();
        let mut front_pixels = [0xFF00_FF00u32; 16];
        let front_surface = Surface::new(&mut front_pixels, 4, 4).unwrap();
        let back = Window::new(1, Rect::new(1, 1, 4, 4));
        let front = Window::new(2, Rect::new(3, 3, 4, 4));
        let layers = [Layer::new(&back, &back_surface), Layer::new(&front, &front_surface)];

        Compositor::new(0xFF00_0000).compose(&mut framebuffer, &layers, Rect::new(0, 0, 8, 8));

        assert_eq!(framebuffer.pixel(2, 2), Some(0xFFFF_0000));
        assert_eq!(framebuffer.pixel(3, 3), Some(0xFF00_FF00));
        assert_eq!(framebuffer.pixel(0, 0), Some(0xFF00_0000));
    }

    #[test]
    fn damage_limits_composition() {
        let mut framebuffer_pixels = [0u32; 64];
        let mut framebuffer = Surface::new(&mut framebuffer_pixels, 8, 8).unwrap();
        let mut pixels = [0xFFFF_FFFFu32; 16];
        let surface = Surface::new(&mut pixels, 4, 4).unwrap();
        let window = Window::new(1, Rect::new(2, 2, 4, 4));
        let layers = [Layer::new(&window, &surface)];

        Compositor::new(0xFF11_2233).compose(&mut framebuffer, &layers, Rect::new(3, 3, 1, 1));

        assert_eq!(framebuffer.pixel(3, 3), Some(0xFFFF_FFFF));
        assert_eq!(framebuffer.pixel(2, 2), Some(0));
    }
}
