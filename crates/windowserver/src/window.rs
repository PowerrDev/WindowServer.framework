/*!
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/*!
 * File:        crates/windowserver/src/window.rs
 *
 * Window state independent of composition, input and server ownership
 */

use crate::geometry::{Point, Rect, Size};
use alloc::string::String;

pub type WindowId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowFlags {
    pub visible: bool,
    pub decorated: bool,
    pub resizable: bool,
    pub opaque: bool,
}

impl Default for WindowFlags {
    fn default() -> Self {
        Self { visible: true, decorated: true, resizable: true, opaque: true }
    }
}

/* `WindowServer` window
 *
 * `frame` is the complete server-side frame. Surface storage is attached later;
 * keeping geometry separate lets the compositor evolve without changing the
 * window lifecycle API.
*/
#[derive(Debug, Clone)]
pub struct Window {
    id: WindowId,
    frame: Rect,
    min_size: Size,
    title: String,
    max_size: Option<Size>,
    titlebar_height: u32,
    flags: WindowFlags,
}

impl Window {
    pub fn new(id: WindowId, frame: Rect) -> Self {
        Self {
            id,
            frame,
            min_size: Size::new(1, 1),
            title: String::from("Untitled"),
            max_size: None,
            titlebar_height: 28,
            flags: WindowFlags::default(),
        }
    }

    pub const fn id(&self) -> WindowId { self.id }
    pub const fn frame(&self) -> Rect { self.frame }
    pub const fn position(&self) -> Point { self.frame.origin }
    pub const fn size(&self) -> Size { self.frame.size }
    pub const fn flags(&self) -> WindowFlags { self.flags }
    pub fn title(&self) -> &str { &self.title }
    pub fn set_title(&mut self, title: impl Into<String>) { self.title = title.into(); }
    pub const fn titlebar_height(&self) -> u32 { self.titlebar_height }

    pub fn set_position(&mut self, position: Point) { self.frame.origin = position; }

    pub fn set_frame(&mut self, frame: Rect) {
        self.frame = Rect { origin: frame.origin, size: self.clamp_size(frame.size) };
    }

    pub fn set_size(&mut self, size: Size) { self.frame.size = self.clamp_size(size); }

    pub fn set_min_size(&mut self, size: Size) {
        self.min_size = size;
        self.frame.size = self.clamp_size(self.frame.size);
    }

    pub fn set_max_size(&mut self, size: Option<Size>) {
        self.max_size = size;
        self.frame.size = self.clamp_size(self.frame.size);
    }

    pub fn set_titlebar_height(&mut self, height: u32) { self.titlebar_height = height; }
    pub fn set_flags(&mut self, flags: WindowFlags) { self.flags = flags; }

    pub fn content_rect(&self) -> Rect {
        let top = self.titlebar_height.min(self.frame.height());
        Rect::new(
            self.frame.x(),
            self.frame.y().saturating_add(top as i32),
            self.frame.width(),
            self.frame.height() - top,
        )
    }

    fn clamp_size(&self, mut size: Size) -> Size {
        size.width = size.width.max(self.min_size.width);
        size.height = size.height.max(self.min_size.height);
        if let Some(max) = self.max_size {
            size.width = size.width.min(max.width);
            size.height = size.height.min(max.height);
        }
        size
    }
}
