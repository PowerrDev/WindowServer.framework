#![allow(dead_code)]

/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */

/**
 * File:        crates/windowserver/src/window.rs
 *
 * Window state independent of rendering and input policy.
 */

use crate::geometry::{Point, Rect, Size};

pub type WindowId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowFlags {
    pub visible: bool,
    pub resizable: bool,
    pub opaque: bool,
    /// The desktop background layer: always full-screen, always at the
    /// bottom of the stack. `WindowServer::focus_window` refuses to focus or
    /// raise a window with this set, so clicking empty desktop space can
    /// never bury whatever app window was on top -- see that method for the
    /// full story.
    pub is_background: bool,
    /// The client's own corner radius (WindowServer draws no chrome of its
    /// own -- see `window::Window`'s doc comment -- but the compositor's drop
    /// shadow needs this to curve its corners the same way the client's own
    /// `fill_rounded_rect` does, or a square shadow corner would show past a
    /// visibly rounded window). 0 for a window that is not rounded (the
    /// desktop background; any client that never set this).
    pub corner_radius: u32,
    /// The drop shadow it casts (see `compositor::ShadowStyle`).
    pub shadow: crate::compositor::ShadowStyle,
    /// The key window: its shadow is the darker one, as on macOS. Set by the
    /// client (which knows which window is key), not by focus here.
    pub key: bool,
    /// Not `opaque`, but opaque everywhere except its rounded corners (an
    /// app window): the rows between the corners are copied whole instead of
    /// tested pixel by pixel.
    pub opaque_interior: bool,
}

impl Default for WindowFlags {
    fn default() -> Self {
        Self {
            visible: true,
            resizable: true,
            opaque: true,
            is_background: false,
            corner_radius: 0,
            shadow: crate::compositor::ShadowStyle::Window,
            key: false,
            opaque_interior: false,
        }
    }
}

/// WindowServer-managed window metadata.
///
/// Visual appearance and client UI are owned by Aqua/UIService.
#[derive(Debug, Clone)]
pub struct Window {
    id: WindowId,
    frame: Rect,
    min_size: Size,
    max_size: Option<Size>,
    flags: WindowFlags,
}

impl Window {
    pub fn new(id: WindowId, frame: Rect) -> Self {
        Self {
            id,
            frame,
            min_size: Size::new(1, 1),
            max_size: None,
            flags: WindowFlags::default(),
        }
    }

    pub const fn id(&self) -> WindowId { self.id }
    pub const fn frame(&self) -> Rect { self.frame }
    pub const fn position(&self) -> Point { self.frame.origin }
    pub const fn size(&self) -> Size { self.frame.size }
    pub const fn flags(&self) -> WindowFlags { self.flags }
    pub const fn max_size(&self) -> Option<Size> { self.max_size }

    pub fn set_position(&mut self, position: Point) {
        self.frame.origin = position;
    }

    pub fn set_frame(&mut self, frame: Rect) {
        self.frame = Rect {
            origin: frame.origin,
            size: self.clamp_size(frame.size),
        };
    }

    pub fn set_size(&mut self, size: Size) {
        self.frame.size = self.clamp_size(size);
    }

    pub fn set_min_size(&mut self, size: Size) {
        self.min_size = size;
        self.frame.size = self.clamp_size(self.frame.size);
    }

    pub fn set_max_size(&mut self, size: Option<Size>) {
        self.max_size = size;
        self.frame.size = self.clamp_size(self.frame.size);
    }

    pub fn set_flags(&mut self, flags: WindowFlags) {
        self.flags = flags;
    }

    pub fn set_opaque(&mut self, opaque: bool) {
        self.flags.opaque = opaque;
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
