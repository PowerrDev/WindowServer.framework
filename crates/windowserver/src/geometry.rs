/*!
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/*!
 * File:        crates/windowserver/src/geometry.rs
 *
 * Fundamental geometry primitives shared by WindowServer.
 */

/* A two-dimensional point in display coordinates. */
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self { Self { x, y } }

    pub const fn offset(self, dx: i32, dy: i32) -> Self {
        Self { x: self.x.saturating_add(dx), y: self.y.saturating_add(dy) }
    }
}

// An unsigned size.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self { Self { width, height } }
    pub const fn is_empty(self) -> bool { self.width == 0 || self.height == 0 }
}

/* An axis-aligned rectangle using a signed origin and unsigned extent
 * 
 * Rectangles are half-open: `[x, x + width) × [y, y + height)`.
*/
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { origin: Point::new(x, y), size: Size::new(width, height) }
    }

    pub const fn empty() -> Self { Self::new(0, 0, 0, 0) }
    pub const fn x(self) -> i32 { self.origin.x }
    pub const fn y(self) -> i32 { self.origin.y }
    pub const fn width(self) -> u32 { self.size.width }
    pub const fn height(self) -> u32 { self.size.height }
    pub const fn is_empty(self) -> bool { self.size.is_empty() }

    pub fn right(self) -> i64 { i64::from(self.origin.x) + i64::from(self.size.width) }
    pub fn bottom(self) -> i64 { i64::from(self.origin.y) + i64::from(self.size.height) }

    pub fn contains_point(self, point: Point) -> bool {
        !self.is_empty()
            && i64::from(point.x) >= i64::from(self.origin.x)
            && i64::from(point.y) >= i64::from(self.origin.y)
            && i64::from(point.x) < self.right()
            && i64::from(point.y) < self.bottom()
    }

    pub fn intersects(self, other: Self) -> bool {
        !self.is_empty() && !other.is_empty()
            && i64::from(self.origin.x) < other.right()
            && i64::from(other.origin.x) < self.right()
            && i64::from(self.origin.y) < other.bottom()
            && i64::from(other.origin.y) < self.bottom()
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        if !self.intersects(other) { return None; }
        let left = i64::from(self.origin.x).max(i64::from(other.origin.x));
        let top = i64::from(self.origin.y).max(i64::from(other.origin.y));
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Some(Self::new(left as i32, top as i32, (right-left) as u32, (bottom-top) as u32))
    }

    pub fn union(self, other: Self) -> Self {
        if self.is_empty() { return other; }
        if other.is_empty() { return self; }
        let left = i64::from(self.origin.x).min(i64::from(other.origin.x));
        let top = i64::from(self.origin.y).min(i64::from(other.origin.y));
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self::new(left as i32, top as i32, (right-left) as u32, (bottom-top) as u32)
    }

    pub fn translated(self, dx: i32, dy: i32) -> Self {
        Self { origin: self.origin.offset(dx, dy), size: self.size }
    }

            // Returns the center point of the rectangle.
    pub fn center(self) -> Point {
        Point::new(
            self.x().saturating_add((self.width() / 2) as i32),
            self.y().saturating_add((self.height() / 2) as i32),
        )
    }

    // Returns true when this rectangle completely contains another rectangle.
    pub fn contains_rect(
        self,
        other: Self,
    ) -> bool {
        if self.is_empty() || other.is_empty() {
            return false;
        }

        i64::from(other.x()) >= i64::from(self.x())
            && i64::from(other.y()) >= i64::from(self.y())
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }

    // Returns a rectangle inset by the given horizontal and vertical amounts.
    //
    // If the inset would make the rectangle empty, an empty rectangle is
    // returned at the resulting origin.
    pub fn inset(
        self,
        dx: u32,
        dy: u32,
    ) -> Self {
        let horizontal = dx.saturating_mul(2);
        let vertical = dy.saturating_mul(2);

        let width = self.width().saturating_sub(horizontal);
        let height = self.height().saturating_sub(vertical);

        Self::new(
            self.x().saturating_add(dx as i32),
            self.y().saturating_add(dy as i32),
            width,
            height,
        )
    }
}
