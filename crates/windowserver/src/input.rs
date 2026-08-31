/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */

/**
 * File:        crates/windowserver/src/input.rs
 *
 * WindowServer pointer input primitives.
 *
 * This module is platform-independent. NXU or another platform backend is
 * responsible for translating hardware events into these logical events.
 */

use crate::geometry::Point;
use crate::window::WindowId;

/// Mouse/pointer buttons understood by WindowServer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
}

/// Logical pointer events delivered to WindowServer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvent {
    Move {
        position: Point,
    },

    ButtonDown {
        position: Point,
        button: PointerButton,
    },

    ButtonUp {
        position: Point,
        button: PointerButton,
    },
}

/// Result returned after routing a pointer event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerResult {
    pub position: Point,
    pub target: Option<WindowId>,
    pub focused: Option<WindowId>,
    pub button: Option<PointerButton>,
    pub pressed: bool,
}

impl PointerResult {
    pub const fn none(position: Point) -> Self {
        Self {
            position,
            target: None,
            focused: None,
            button: None,
            pressed: false,
        }
    }
}

/// Mutable pointer state owned by WindowServer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerState {
    position: Point,
    pressed_button: Option<PointerButton>,
    captured_window: Option<WindowId>,
}

impl PointerState {
    pub const fn new(position: Point) -> Self {
        Self {
            position,
            pressed_button: None,
            captured_window: None,
        }
    }

    pub const fn position(&self) -> Point {
        self.position
    }

    pub const fn pressed_button(&self) -> Option<PointerButton> {
        self.pressed_button
    }

    pub const fn captured_window(&self) -> Option<WindowId> {
        self.captured_window
    }

    pub fn set_position(&mut self, position: Point) {
        self.position = position;
    }

    pub fn press(&mut self, button: PointerButton, window: Option<WindowId>) {
        self.pressed_button = Some(button);
        self.captured_window = window;
    }

    pub fn release(&mut self) {
        self.pressed_button = None;
        self.captured_window = None;
    }
}