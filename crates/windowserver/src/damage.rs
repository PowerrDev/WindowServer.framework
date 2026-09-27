/*!
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/*!
 * File:        crates/windowserver/src/damage.rs
 *
 * Damage tracking: a small bounded list of dirty rectangles rather than one
 * conservative union.
 *
 * A single union rect is what made every window move recomposite close to
 * the window's *entire* area regardless of how far it actually moved: the
 * union of a large rect with itself shifted by a few pixels is still nearly
 * that whole rect. Keeping several distinct rects lets a caller (see
 * `WindowServer::apply_pending_scrolls`) damage only the thin strips that
 * actually changed. Capped rather than unbounded so a pathological case
 * (many small, scattered damages in one frame) can't make the damage list
 * itself the bottleneck -- it degrades to the old single-union behavior
 * instead.
 */

use alloc::vec::Vec;

use crate::geometry::Rect;

const MAX_RECTS: usize = 16;

#[derive(Debug, Clone, Default)]
pub struct Damage {
    rects: Vec<Rect>,
    /// Set once the list would exceed `MAX_RECTS`: from then on, `rects`
    /// holds exactly one conservative union rect instead of growing further
    /// or paying to de-overlap many small ones.
    overflowed: bool,
}

impl Damage {
    pub fn new() -> Self {
        Self { rects: Vec::new(), overflowed: false }
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// The bounding rect of everything currently damaged, e.g. for a single
    /// display-flush call that doesn't need per-rect granularity.
    pub fn rect(&self) -> Option<Rect> {
        self.rects.iter().copied().reduce(Rect::union)
    }

    pub fn clear(&mut self) {
        self.rects.clear();
        self.overflowed = false;
    }

    pub fn add(&mut self, rect: Rect) {
        if rect.is_empty() {
            return;
        }

        if self.overflowed {
            self.rects[0] = self.rects[0].union(rect);
            return;
        }

        // Each rect is composited on its own, so overlapping ones redraw
        // their overlap once per rect: a resize damages the old frame, the
        // new one and whatever the redraws changed, nearly the same area
        // three times. A rect merges with any it overlaps enough that their
        // union is no bigger than the two apart (one inside the other is
        // the extreme case); far-apart rects and the thin strips a move
        // damages stay separate.
        let mut rect = rect;
        let mut index = 0;
        while index < self.rects.len() {
            let other = self.rects[index];
            let union = other.union(rect);
            if area(union) <= area(other) + area(rect) {
                rect = union;
                self.rects.swap_remove(index);
                index = 0;
                continue;
            }
            index += 1;
        }

        if self.rects.len() >= MAX_RECTS {
            let unioned = self.rects.iter().copied().fold(rect, Rect::union);
            self.rects.clear();
            self.rects.push(unioned);
            self.overflowed = true;
            return;
        }

        self.rects.push(rect);
    }

    /// Take every damaged rect (overlapping ones merged), leaving this
    /// `Damage` empty. Callers composite each one independently -- see
    /// `WindowServer::present`.
    pub fn take(&mut self) -> Vec<Rect> {
        self.overflowed = false;
        core::mem::take(&mut self.rects)
    }

    pub fn intersects(&self, rect: Rect) -> bool {
        self.rects.iter().any(|damaged| damaged.intersects(rect))
    }
}

fn area(rect: Rect) -> u64 {
    rect.size.width as u64 * rect.size.height as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_rects_stay_distinct_instead_of_unioning() {
        let mut damage = Damage::new();
        damage.add(Rect::new(0, 0, 10, 10));
        damage.add(Rect::new(1000, 1000, 10, 10));

        let rects = damage.take();
        assert_eq!(rects.len(), 2, "two far-apart rects should not get merged into one giant union");
    }

    #[test]
    fn overlapping_rects_merge_instead_of_redrawing_the_overlap() {
        let mut damage = Damage::new();
        // A window's old frame, its slightly smaller new one, and a redraw inside it.
        damage.add(Rect::new(100, 100, 400, 300));
        damage.add(Rect::new(100, 100, 390, 290));
        damage.add(Rect::new(120, 150, 200, 100));
        assert_eq!(damage.take(), alloc::vec![Rect::new(100, 100, 400, 300)]);

        // Two strips along a moved window's edges would union into their whole
        // bounding box: they stay apart.
        damage.add(Rect::new(0, 0, 400, 10));
        damage.add(Rect::new(0, 0, 10, 300));
        assert_eq!(damage.take().len(), 2);
    }

    #[test]
    fn empty_rects_are_ignored() {
        let mut damage = Damage::new();
        damage.add(Rect::empty());
        assert!(damage.is_empty());
    }

    #[test]
    fn overflow_falls_back_to_one_conservative_union() {
        let mut damage = Damage::new();
        for i in 0..(MAX_RECTS as i32 + 5) {
            damage.add(Rect::new(i * 20, 0, 5, 5));
        }

        let rects = damage.take();
        assert_eq!(rects.len(), 1, "past the cap, damage should degrade to a single rect rather than grow unboundedly");
    }

    #[test]
    fn take_clears_the_list() {
        let mut damage = Damage::new();
        damage.add(Rect::new(0, 0, 1, 1));
        let _ = damage.take();
        assert!(damage.is_empty());
        assert!(damage.take().is_empty());
    }
}
