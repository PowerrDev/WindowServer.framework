/*!
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/*! Server-side window decorations and titlebar hit testing. */

use crate::{geometry::{Point, Rect}, surface::Surface, window::Window};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowHit { None, TitleBar, Content, CloseButton }

pub struct Decorations;

impl Decorations {
    pub const BORDER: u32 = 1;
    pub const BUTTON_SIZE: u32 = 12;
    pub const BUTTON_MARGIN: u32 = 8;

    pub fn hit_test(window: &Window, point: Point) -> WindowHit {
        let frame = window.frame();
        if !frame.contains_point(point) { return WindowHit::None; }
        if !window.flags().decorated { return WindowHit::Content; }
        let title = Self::titlebar_rect(window);
        if title.contains_point(point) {
            if Self::close_button_rect(window).contains_point(point) { WindowHit::CloseButton } else { WindowHit::TitleBar }
        } else { WindowHit::Content }
    }

    pub fn titlebar_rect(window: &Window) -> Rect {
        let frame = window.frame();
        Rect::new(frame.x(), frame.y(), frame.width(), window.titlebar_height().min(frame.height()))
    }

    pub fn close_button_rect(window: &Window) -> Rect {
        let title = Self::titlebar_rect(window);
        let size = Self::BUTTON_SIZE.min(title.height());
        let x = title.right() as i32 - Self::BUTTON_MARGIN as i32 - size as i32;
        let y = title.y() + (title.height().saturating_sub(size) / 2) as i32;
        Rect::new(x, y, size, size)
    }

    pub fn draw(window: &Window, surface: &mut Surface<'_>, focused: bool) {
        if !window.flags().decorated { return; }
        let frame = window.frame();
        let title = Self::titlebar_rect(window);
        let title_pixel = if focused { 0xFF3A_3A3C } else { 0xFF5A_5A5E };
        Self::fill_signed(surface, title, title_pixel);
        // Thin frame outline.
        Self::fill_signed(surface, Rect::new(frame.x(), frame.y(), frame.width(), Self::BORDER), 0xFF10_1010);
        if frame.height() > 1 { Self::fill_signed(surface, Rect::new(frame.x(), frame.bottom() as i32 - 1, frame.width(), 1), 0xFF10_1010); }
        Self::fill_signed(surface, Self::close_button_rect(window), 0xFFFF_5F57);
    }

    fn fill_signed(surface: &mut Surface<'_>, rect: Rect, pixel: u32) {
        let left = rect.x().max(0) as u32; let top = rect.y().max(0) as u32;
        let right = rect.right().min(surface.width() as i64).max(0) as u32;
        let bottom = rect.bottom().min(surface.height() as i64).max(0) as u32;
        if right > left && bottom > top { surface.fill_rect(left, top, right-left, bottom-top, pixel); }
    }
}
