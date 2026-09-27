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

// Mouse/pointer buttons understood by WindowServer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
}

// Logical pointer events delivered to WindowServer.
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

// Result returned after routing a pointer event.
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

// Mutable pointer state owned by WindowServer.
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

    pub fn press(
        &mut self,
        button: PointerButton,
        window: Option<WindowId>,
    ) {
        self.pressed_button = Some(button);
        self.captured_window = window;
    }

    pub fn release(&mut self) {
        self.pressed_button = None;
        self.captured_window = None;
    }
}

/// A "shake to locate the pointer" gesture: a lot of back-and-forth travel
/// packed into a small area.
///
/// This accumulates cumulative path length and a bounding box since the last
/// reset, instead of over a fixed-size *sample* window: a fixed sample count
/// only works if every platform delivers roughly the same distance per
/// packet, which does not hold in practice -- a coarse synthetic jump and a
/// real mouse's/trackpad's naturally fine-grained packets can differ by more
/// than an order of magnitude, and a window sized for one badly undercounts
/// the other (a real shake's packets can "age out" of a short window before
/// enough distance ever accumulates). Tracking distance directly is
/// independent of how finely the motion happens to be packetized.
///
/// `PATH_THRESHOLD`/`AREA_LIMIT` are physical-pixel heuristics tuned by feel
/// on a 2x-density display (see `WindowStyle`'s doubling convention); they
/// may need retuning against a real mouse. `MAX_STEPS` bounds how long slow,
/// non-shake meandering within a small area can accumulate path credit --
/// a genuine shake crosses `PATH_THRESHOLD` in far fewer steps than this
/// regardless of packet granularity, so this only caps the "memory" for
/// anything slower (idle jitter, hovering) from eventually mistriggering.
#[derive(Debug, Clone, Copy)]
pub struct ShakeDetector {
    anchor: Option<Point>,
    last: Point,
    min: Point,
    max: Point,
    path: i64,
    steps: u32,
}

impl ShakeDetector {
    const PATH_THRESHOLD: i64 = 900;
    const AREA_LIMIT: i64 = 260;
    const MAX_STEPS: u32 = 200;

    pub const fn new() -> Self {
        Self {
            anchor: None,
            last: Point::new(0, 0),
            min: Point::new(0, 0),
            max: Point::new(0, 0),
            path: 0,
            steps: 0,
        }
    }

    /// Drop any in-progress history, e.g. when a button press starts a drag
    /// (shaking a window while dragging it should not trigger this).
    pub fn clear(&mut self) {
        self.anchor = None;
        self.path = 0;
        self.steps = 0;
    }

    fn reset_to(&mut self, position: Point) {
        self.anchor = Some(position);
        self.last = position;
        self.min = position;
        self.max = position;
        self.path = 0;
        self.steps = 0;
    }

    /// Record a pointer move. Returns `true` the instant accumulated path
    /// completes a shake; the history is cleared immediately after so the
    /// same shake cannot be reported twice.
    pub fn record(&mut self, position: Point) -> bool {
        if self.anchor.is_none() {
            self.reset_to(position);
            return false;
        }

        self.path += (position.x - self.last.x).unsigned_abs() as i64
            + (position.y - self.last.y).unsigned_abs() as i64;
        self.last = position;
        self.min.x = self.min.x.min(position.x);
        self.min.y = self.min.y.min(position.y);
        self.max.x = self.max.x.max(position.x);
        self.max.y = self.max.y.max(position.y);
        self.steps += 1;

        let area = (self.max.x - self.min.x) as i64 + (self.max.y - self.min.y) as i64;

        // Wandered out of a "local" area, or lingered too long without
        // resolving either way: neither is a shake in progress, so start
        // tracking fresh from here instead of carrying stale path credit.
        if area > Self::AREA_LIMIT || self.steps > Self::MAX_STEPS {
            self.reset_to(position);
            return false;
        }

        if self.path >= Self::PATH_THRESHOLD {
            self.clear();
            true
        } else {
            false
        }
    }
}

impl Default for ShakeDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod shake_tests {
    use super::*;

    #[test]
    fn rapid_back_and_forth_in_place_is_a_shake() {
        let mut shake = ShakeDetector::new();
        let a = Point::new(1000, 500);
        let b = Point::new(1130, 500);

        // Oscillate until accumulated path crosses PATH_THRESHOLD; must not
        // fire before then.
        let mut triggered = false;
        for i in 0..20 {
            let point = if i % 2 == 0 { a } else { b };
            if shake.record(point) {
                triggered = true;
                break;
            }
        }
        assert!(triggered, "sustained oscillation in a small area must eventually read as a shake");
    }

    #[test]
    fn fine_grained_packets_still_detect_a_shake() {
        // A real mouse/trackpad reports many small deltas rather than one
        // big jump per direction change -- this is what the fixed-sample-
        // window design used to miss entirely.
        let mut shake = ShakeDetector::new();
        let mut x = 1000i32;
        let mut triggered = false;

        for i in 0..200 {
            x += if i % 2 == 0 { 10 } else { -10 };
            if shake.record(Point::new(x, 500)) {
                triggered = true;
                break;
            }
        }
        assert!(triggered, "fine-grained oscillating packets must still accumulate into a shake");
    }

    #[test]
    fn fast_pan_across_the_screen_is_not_a_shake() {
        let mut shake = ShakeDetector::new();

        // A lot of cumulative travel, same as a shake, but in one direction
        // -- the bounding box keeps widening past AREA_LIMIT, so this must
        // never trigger no matter how long it runs.
        for step in 0..500 {
            let result = shake.record(Point::new(step as i32 * 5, 0));
            assert!(!result, "a straight pan must not read as a shake");
        }
    }

    #[test]
    fn clear_forgets_in_progress_history() {
        let mut shake = ShakeDetector::new();
        shake.record(Point::new(0, 0));
        shake.record(Point::new(1, 0));
        shake.clear();

        // After clearing, the detector needs a fresh anchor again -- it must
        // not fire on a single follow-up sample.
        assert!(!shake.record(Point::new(2, 0)));
    }
}
