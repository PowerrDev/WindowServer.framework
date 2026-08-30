/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/**
 * File:        crates/windowserver/src/server.rs
 *
 * WindowServer state management.
 *
 * The server owns window lifecycle, backing storage, Z-order and damage.
 * Platform-specific framebuffer presentation remains outside this crate.
 */

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use crate::{
    compositor::{Compositor, Layer},
    damage::Damage,
    geometry::{Point, Rect},
    surface::{Surface, SurfaceError},
    window::{Window, WindowId},
};

struct ServerWindow {
    window: Window,
    pixels: Box<[u32]>,
    stride: u32,
}


/// Platform-independent WindowServer core.
///
/// Windows are stored in back-to-front order. The last window is therefore the
/// frontmost window and naturally wins during composition.
pub struct WindowServer {
    compositor: Compositor,
    windows: Vec<ServerWindow>,
    damage: Damage,
    next_window_id: WindowId,
}

impl WindowServer {
    pub fn new(background: u32) -> Self {
        Self {
            compositor: Compositor::new(background),
            windows: Vec::new(),
            damage: Damage::new(),
            next_window_id: 1,
        }
    }

    pub fn compositor(&self) -> &Compositor { &self.compositor }
    pub fn compositor_mut(&mut self) -> &mut Compositor { &mut self.compositor }
    pub fn window_count(&self) -> usize { self.windows.len() }
    pub fn damage(&self) -> &Damage { &self.damage }

    pub fn create_window(&mut self, frame: Rect) -> Result<WindowId, SurfaceError> {
        let pixels = pixel_len(frame.width(), frame.height())?;
        let id = self.allocate_window_id();
        self.windows.push(ServerWindow {
            window: Window::new(id, frame),
            pixels: vec![0; pixels].into_boxed_slice(),
            stride: frame.width(),
        });
        self.damage.add(frame);
        Ok(id)
    }

    pub fn destroy_window(&mut self, id: WindowId) -> bool {
        let Some(index) = self.window_index(id) else { return false; };
        let frame = self.windows[index].window.frame();
        self.windows.remove(index);
        self.damage.add(frame);
        true
    }

    pub fn window(&self, id: WindowId) -> Option<&Window> {
        self.windows.get(self.window_index(id)?).map(|entry| &entry.window)
    }

    pub fn window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        let index = self.window_index(id)?;
        Some(&mut self.windows[index].window)
    }

    pub fn move_window(&mut self, id: WindowId, position: Point) -> bool {
        let Some(index) = self.window_index(id) else { return false; };
        let window = &mut self.windows[index].window;
        let old = window.frame();
        window.set_position(position);
        self.damage.add(old);
        self.damage.add(window.frame());
        true
    }

    pub fn bring_to_front(&mut self, id: WindowId) -> bool {
        let Some(index) = self.window_index(id) else { return false; };
        if index + 1 == self.windows.len() { return true; }

        let frame = self.windows[index].window.frame();
        let entry = self.windows.remove(index);
        self.windows.push(entry);
        self.damage.add(frame);
        true
    }

    pub fn fill_window(&mut self, id: WindowId, pixel: u32) -> bool {
        let Some(index) = self.window_index(id) else { return false; };
        let frame = self.windows[index].window.frame();
        self.windows[index].pixels.fill(pixel);
        self.damage.add(frame);
        true
    }

    pub fn present(&mut self, framebuffer: &mut Surface<'_>) -> Result<bool, SurfaceError> {
        let Some(damage) = self.damage.take() else { return Ok(false); };

        self.compositor.begin(framebuffer, damage);

        for entry in &mut self.windows {
            let ServerWindow { window, pixels, stride } = entry;
            let surface = Surface::with_stride(
                pixels,
                window.frame().width(),
                window.frame().height(),
                *stride,
            )?;
            let layer = Layer::new(window, &surface);
            self.compositor.compose_layer(framebuffer, &layer, damage);
        }

        Ok(true)
    }

    fn window_index(&self, id: WindowId) -> Option<usize> {
        self.windows.iter().position(|entry| entry.window.id() == id)
    }

    fn allocate_window_id(&mut self) -> WindowId {
        let id = self.next_window_id;
        self.next_window_id = self.next_window_id.wrapping_add(1);
        if self.next_window_id == 0 { self.next_window_id = 1; }
        id
    }
}

fn pixel_len(width: u32, height: u32) -> Result<usize, SurfaceError> {
    if width == 0 || height == 0 {
        return Err(SurfaceError::InvalidDimensions);
    }

    (width as usize)
        .checked_mul(height as usize)
        .ok_or(SurfaceError::InvalidDimensions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_window_composes_over_back_window() {
        let mut server = WindowServer::new(0xFF00_0000);
        let back = server.create_window(Rect::new(1, 1, 4, 4)).unwrap();
        let front = server.create_window(Rect::new(3, 3, 4, 4)).unwrap();
        server.fill_window(back, 0xFFFF_0000);
        server.fill_window(front, 0xFF00_FF00);

        let mut pixels = [0u32; 64];
        let mut framebuffer = Surface::new(&mut pixels, 8, 8).unwrap();
        assert!(server.present(&mut framebuffer).unwrap());

        assert_eq!(framebuffer.pixel(2, 2), Some(0xFFFF_0000));
        assert_eq!(framebuffer.pixel(3, 3), Some(0xFF00_FF00));
    }

    #[test]
    fn moving_window_damages_old_and_new_frames() {
        let mut server = WindowServer::new(0xFF00_0000);
        let id = server.create_window(Rect::new(1, 1, 2, 2)).unwrap();
        let _ = server.damage.take();

        assert!(server.move_window(id, Point::new(5, 1)));
        assert_eq!(server.damage.rect(), Some(Rect::new(1, 1, 6, 2)));
    }

    #[test]
    fn bring_to_front_changes_z_order() {
        let mut server = WindowServer::new(0xFF00_0000);
        let first = server.create_window(Rect::new(1, 1, 4, 4)).unwrap();
        let second = server.create_window(Rect::new(1, 1, 4, 4)).unwrap();
        server.fill_window(first, 0xFFFF_0000);
        server.fill_window(second, 0xFF00_FF00);
        assert!(server.bring_to_front(first));

        let mut pixels = [0u32; 64];
        let mut framebuffer = Surface::new(&mut pixels, 8, 8).unwrap();
        server.present(&mut framebuffer).unwrap();

        assert_eq!(framebuffer.pixel(2, 2), Some(0xFFFF_0000));
    }
}
