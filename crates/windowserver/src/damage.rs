/*!
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/*!
 * File:        crates/windowserver/src/damage.rs
 *
 * Minimal damage tracking. The compositor currently consumes one conservative
 * rectangle; region lists can be introduced later without changing callers
 */

use crate::geometry::Rect;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Damage {
    rect: Option<Rect>,
}

impl Damage {
    pub const fn new() -> Self { Self { rect: None } }
    pub const fn is_empty(&self) -> bool { self.rect.is_none() }
    pub const fn rect(&self) -> Option<Rect> { self.rect }

    pub fn clear(&mut self) { self.rect = None; }

    pub fn add(&mut self, rect: Rect) {
        if rect.is_empty() { return; }
        self.rect = Some(match self.rect {
            Some(existing) => existing.union(rect),
            None => rect,
        });
    }

    pub fn take(&mut self) -> Option<Rect> {
        let rect = self.rect;
        self.rect = None;
        rect
    }

    pub fn intersects(&self, rect: Rect) -> bool {
        self.rect.is_some_and(|damage| damage.intersects(rect))
    }
}
