/*!
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/*! Software cursor primitives. */

use alloc::boxed::Box;

use crate::{animation::EasedScale, compositor::blend, geometry::{Point, Rect}, surface::Surface};

/// A decoded pointer bitmap installed by Aqua/UIService, in premultiplied-free
/// straight-alpha ARGB8888 (matching every other surface in this crate).
struct CursorBitmap {
    pixels: Box<[u32]>,
    width: u32,
    height: u32,
    stride: u32,
    hotspot: Point,
}

/// What the pointer is doing, decided by whoever knows: WindowServer itself
/// for window chrome it manages directly (dragging a titlebar, an edge being
/// resized), Aqua/UIService for everything inside a window's content it
/// hit-tests itself (a button, a text link, a control that refuses input
/// right now) -- see `WS_Set_Cursor_Kind`. Each kind shows its own installed
/// bitmap (`set_bitmap`, one slot per kind) if one was installed for it, or
/// its own built-in fallback shape (`draw`) otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorKind {
    #[default]
    Arrow,
    /// Dragging or moving a window.
    Move,
    /// Over something that responds to a click: a titlebar, a button, a link.
    Hand,
    /// Over something that refuses input right now.
    NotAllowed,
    /// Over a left or right resize edge.
    ResizeHorizontal,
    /// Over a top or bottom resize edge.
    ResizeVertical,
    /// Over a top-right or bottom-left resize corner (the "/" diagonal).
    ResizeDiagonalNeSw,
    /// Over a top-left or bottom-right resize corner (the "\" diagonal).
    ResizeDiagonalNwSe,
    /// Over editable or selectable text.
    Text,
}

impl CursorKind {
    /// How many kinds there are, and this kind's own slot among them (see
    /// `Cursor`'s per-kind bitmap array) -- must stay in sync with the
    /// variant list above; a mismatch panics (out-of-bounds array access)
    /// immediately in every test that touches a `Cursor`, not silently.
    pub const COUNT: usize = 9;

    const fn index(self) -> usize {
        match self {
            Self::Arrow => 0,
            Self::Move => 1,
            Self::Hand => 2,
            Self::NotAllowed => 3,
            Self::ResizeHorizontal => 4,
            Self::ResizeVertical => 5,
            Self::ResizeDiagonalNeSw => 6,
            Self::ResizeDiagonalNwSe => 7,
            Self::Text => 8,
        }
    }
}

pub struct Cursor {
    position: Point,
    // One slot per CursorKind (CursorKind::index): each kind shows its own
    // installed bitmap if it has one, independent of every other kind's --
    // switching kinds never disturbs what was installed for a different one.
    bitmaps: [Option<CursorBitmap>; CursorKind::COUNT],
    kind: CursorKind,
    // "Shake to locate the pointer": grows while shaken, then eases back.
    // Only affects an installed bitmap -- see `draw_fallback`.
    scale: EasedScale,
}

impl Cursor {
    // Fallback shape size only; an installed bitmap uses its own dimensions.
    pub const WIDTH: u32 = 12;
    pub const HEIGHT: u32 = 18;

    // The non-Arrow fallback shapes are drawn on a fixed square this size,
    // same technique as the arrow's own SHAPE grid below.
    const KIND_SHAPE_SIZE: u32 = 16;

    const SHAKE_PEAK_SCALE: u32 = EasedScale::ONE * 3;

    pub const fn new(position: Point) -> Self {
        const NONE: Option<CursorBitmap> = None;
        Self {
            position,
            bitmaps: [NONE; CursorKind::COUNT],
            kind: CursorKind::Arrow,
            scale: EasedScale::new(),
        }
    }
    pub const fn position(&self) -> Point { self.position }
    pub fn set_position(&mut self, position: Point) { self.position = position; }

    pub const fn kind(&self) -> CursorKind { self.kind }

    /// Change what the pointer shows. Returns `false` (nothing changed) if
    /// `kind` was already current.
    pub fn set_kind(&mut self, kind: CursorKind) -> bool {
        if self.kind == kind {
            return false;
        }
        self.kind = kind;
        true
    }

    /// Install a real cursor bitmap (e.g. a decoded `.cur` asset) for `kind`,
    /// replacing that kind's own built-in fallback shape whenever it is the
    /// active kind -- every other kind is unaffected. Returns `false`
    /// (bitmap left unchanged) if the dimensions are inconsistent with
    /// `pixels`' length.
    pub fn set_bitmap(
        &mut self,
        kind: CursorKind,
        pixels: &[u32],
        width: u32,
        height: u32,
        stride: u32,
        hotspot: Point,
    ) -> bool {
        if width == 0 || height == 0 || stride < width {
            return false;
        }

        let Some(required) = (stride as usize).checked_mul(height as usize) else {
            return false;
        };
        if pixels.len() < required {
            return false;
        }

        self.bitmaps[kind.index()] = Some(CursorBitmap {
            pixels: pixels[..required].into(),
            width,
            height,
            stride,
            hotspot,
        });
        true
    }

    /// Start the "shake to locate" grow effect: ease up to 3x size, hold
    /// briefly, then ease back to 1x -- see `animation.rs` for the actual
    /// timings, all real wall-clock durations driven by `now_us`.
    pub fn trigger_grow(&mut self, now_us: u64) {
        self.scale.bump(Self::SHAKE_PEAK_SCALE, now_us);
    }

    /// Advance the grow/shrink animation to `now_us`. Returns `true` if the
    /// on-screen size changed, so the caller should invalidate both the
    /// pre- and post-step `frame()`.
    pub fn step_animation(&mut self, now_us: u64) -> bool {
        self.scale.step(now_us)
    }

    fn scaled_size(bitmap: &CursorBitmap, scale: u32) -> (u32, u32) {
        (
            (bitmap.width * scale) / EasedScale::ONE,
            (bitmap.height * scale) / EasedScale::ONE,
        )
    }

    fn scaled_hotspot(bitmap: &CursorBitmap, scale: u32) -> Point {
        Point::new(
            (bitmap.hotspot.x * scale as i32) / EasedScale::ONE as i32,
            (bitmap.hotspot.y * scale as i32) / EasedScale::ONE as i32,
        )
    }

    pub fn frame(&self) -> Rect {
        match &self.bitmaps[self.kind.index()] {
            Some(bitmap) => {
                let (width, height) = Self::scaled_size(bitmap, self.scale.current());
                let hotspot = Self::scaled_hotspot(bitmap, self.scale.current());
                Rect::new(
                    self.position.x - hotspot.x,
                    self.position.y - hotspot.y,
                    width,
                    height,
                )
            }
            None => match self.kind {
                CursorKind::Arrow => Rect::new(self.position.x, self.position.y, Self::WIDTH, Self::HEIGHT),
                _ => Rect::new(
                    self.position.x - (Self::KIND_SHAPE_SIZE / 2) as i32,
                    self.position.y - (Self::KIND_SHAPE_SIZE / 2) as i32,
                    Self::KIND_SHAPE_SIZE,
                    Self::KIND_SHAPE_SIZE,
                ),
            },
        }
    }

    pub fn draw(&self, surface: &mut Surface<'_>) {
        match &self.bitmaps[self.kind.index()] {
            Some(bitmap) => Self::draw_bitmap(surface, self.position, bitmap, self.scale.current()),
            None => match self.kind {
                CursorKind::Arrow | CursorKind::Text => Self::draw_fallback(surface, self.position),
                CursorKind::Hand => Self::draw_fallback_hand(surface, self.position),
                CursorKind::NotAllowed => Self::draw_fallback_not_allowed(surface, self.position),
                // No dedicated fallback art for a specific resize direction:
                // these five real assets (move.cur, resize-*.cur) are the
                // expected, tested path (see cursor.rs's own doc comment on
                // CursorKind), so the fallback only has to be a reasonable
                // "something is being manipulated here" placeholder, not a
                // pixel-perfect direction indicator.
                CursorKind::Move
                | CursorKind::ResizeHorizontal
                | CursorKind::ResizeVertical
                | CursorKind::ResizeDiagonalNeSw
                | CursorKind::ResizeDiagonalNwSe => Self::draw_fallback_move(surface, self.position),
            },
        }
    }

    fn draw_bitmap(surface: &mut Surface<'_>, position: Point, bitmap: &CursorBitmap, scale: u32) {
        let (scaled_width, scaled_height) = Self::scaled_size(bitmap, scale);
        if scaled_width == 0 || scaled_height == 0 {
            return;
        }

        let hotspot = Self::scaled_hotspot(bitmap, scale);
        let origin_x = position.x - hotspot.x;
        let origin_y = position.y - hotspot.y;

        // Nearest-neighbor upscale, not bilinear: pure integer division, no
        // soft-float, and this only ever magnifies a small bitmap in place,
        // where a soft blur would make the pointer harder to read, not
        // easier. At rest (scale == ONE) this reduces to the original 1:1
        // blit exactly, since `source_x == x` and `source_y == y`.
        for y in 0..scaled_height {
            let py = origin_y + y as i32;
            if py < 0 {
                continue;
            }
            let source_y = (y * bitmap.height / scaled_height).min(bitmap.height - 1);
            let row_start = source_y as usize * bitmap.stride as usize;

            for x in 0..scaled_width {
                let px = origin_x + x as i32;
                if px < 0 {
                    continue;
                }
                let source_x = (x * bitmap.width / scaled_width).min(bitmap.width - 1);

                let pixel = bitmap.pixels[row_start + source_x as usize];
                let alpha = pixel >> 24;
                if alpha == 0 {
                    continue;
                }

                let (px, py) = (px as u32, py as u32);
                if alpha == 255 {
                    surface.set_pixel(px, py, 0xFF00_0000 | (pixel & 0x00FF_FFFF));
                } else if let Some(destination) = surface.pixel(px, py) {
                    surface.set_pixel(px, py, blend(destination, pixel));
                }
            }
        }
    }

    // Classic arrow, black outline + white fill. Used only until a real
    // cursor bitmap is installed via `set_bitmap`; deliberately not affected
    // by the shake-to-grow scale, since scaling this ASCII-art shape isn't
    // worth the code for a placeholder that never ships to a real session.
    fn draw_fallback(surface: &mut Surface<'_>, position: Point) {
        const BLACK: u32 = 0xFF00_0000;
        const WHITE: u32 = 0xFFFF_FFFF;
        const SHAPE: [&str; 18] = [
            "B...........", "BW..........", "BWW.........", "BWWW........",
            "BWWWW.......", "BWWWWW......", "BWWWWWW.....", "BWWWWWWW....",
            "BWWWWWWWW...", "BWWWWWWWWW..", "BWWWWWWWWWB.", "BWWWWBWWBB..",
            "BWWWB.BWW...", "BWWB..BWW...", "BWB...BWW...", "BB....BWW...",
            "......BB....", "............",
        ];
        for (y, row) in SHAPE.iter().enumerate() {
            for (x, byte) in row.as_bytes().iter().enumerate() {
                let pixel = match byte { b'B' => BLACK, b'W' => WHITE, _ => continue };
                let px = position.x + x as i32;
                let py = position.y + y as i32;
                if px >= 0 && py >= 0 { surface.set_pixel(px as u32, py as u32, pixel); }
            }
        }
    }

    // `position` is the shape's *center* for every non-Arrow kind (see
    // `frame()`), unlike the Arrow fallback above (whose `position` is a
    // corner, matching a real pointer's hotspot). These three are plain
    // per-pixel distance/offset tests rather than hand-audited ASCII art --
    // easy to get right and to unit-test without a real display to check
    // pixel art against.
    fn set_kind_pixel(surface: &mut Surface<'_>, center: Point, dx: i32, dy: i32, pixel: u32) {
        let px = center.x + dx;
        let py = center.y + dy;
        if px >= 0 && py >= 0 {
            surface.set_pixel(px as u32, py as u32, pixel);
        }
    }

    // Four short triangular arrowheads pointing away from the center, plus a
    // connecting spine: the classic "this can be moved" cross.
    fn draw_fallback_move(surface: &mut Surface<'_>, position: Point) {
        const BLACK: u32 = 0xFF00_0000;
        const WHITE: u32 = 0xFFFF_FFFF;
        const ARM: i32 = 7; // tip distance from center
        const HEAD: i32 = 3; // arrowhead half-width at its base

        // Spine: a thin plus sign connecting all four tips.
        for offset in -ARM..=ARM {
            Self::set_kind_pixel(surface, position, offset, 0, WHITE);
            Self::set_kind_pixel(surface, position, 0, offset, WHITE);
        }

        // One triangular head per direction: `along` grows from the spine
        // out to the tip, `across` is the head's half-width at that point
        // (widest at the base, a point at the tip).
        for &(along_x, along_y, across_x, across_y) in &[
            (0i32, -1i32, 1i32, 0i32),  // up
            (0, 1, 1, 0),               // down
            (-1, 0, 0, 1),              // left
            (1, 0, 0, 1),               // right
        ] {
            for along in (ARM - HEAD)..=ARM {
                let half_width = ARM - along;
                for across in -half_width..=half_width {
                    let dx = along_x * along + across_x * across;
                    let dy = along_y * along + across_y * across;
                    let pixel = if across.abs() == half_width || along == ARM { BLACK } else { WHITE };
                    Self::set_kind_pixel(surface, position, dx, dy, pixel);
                }
            }
        }
    }

    // A pointing hand: a rounded "finger" over a wider "palm", both filled
    // rounded rectangles approximated with a corner cutout.
    fn draw_fallback_hand(surface: &mut Surface<'_>, position: Point) {
        const BLACK: u32 = 0xFF00_0000;
        const WHITE: u32 = 0xFFFF_FFFF;

        // Finger: a narrow vertical bar above the palm, offset left of
        // center so the hotspot (top of the finger) reads as the pointing tip.
        for dy in -7i32..=1 {
            for dx in -2i32..=1 {
                let corner = dy == -7 && (dx == -2 || dx == 1);
                if corner {
                    continue;
                }
                let edge = dx == -2 || dx == 1 || dy == -7;
                Self::set_kind_pixel(surface, position, dx, dy, if edge { BLACK } else { WHITE });
            }
        }

        // Palm: a wider block below the finger, right edge rounded off by a
        // one-pixel corner cut so it doesn't read as a plain brick.
        for dy in 2i32..=7 {
            for dx in -3i32..=5 {
                let corner_cut = dy == 7 && dx == 5;
                if corner_cut {
                    continue;
                }
                let edge = dx == -3 || dx == 5 || dy == 7 || (dy == 2 && dx > 1);
                Self::set_kind_pixel(surface, position, dx, dy, if edge { BLACK } else { WHITE });
            }
        }
    }

    // The universal "not allowed" ring-and-slash, computed the same way
    // `ui-render`'s `fill_circle` does (squared-distance test), no floats.
    fn draw_fallback_not_allowed(surface: &mut Surface<'_>, position: Point) {
        const RED: u32 = 0xFFE0_3030;
        const RADIUS: i32 = 7;
        const THICKNESS: i32 = 2;

        for dy in -RADIUS..=RADIUS {
            for dx in -RADIUS..=RADIUS {
                let distance_sq = dx * dx + dy * dy;
                let outer = RADIUS * RADIUS;
                let inner = (RADIUS - THICKNESS) * (RADIUS - THICKNESS);
                let on_ring = distance_sq <= outer && distance_sq >= inner;
                // The slash is the ring's own diagonal, one pixel wide,
                // so it reads as a single stroke through the circle rather
                // than a second overlapping shape.
                let on_slash = distance_sq <= outer && (dx - dy).abs() <= 1;
                if on_ring || on_slash {
                    Self::set_kind_pixel(surface, position, dx, dy, RED);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn new_surface(size: u32) -> (alloc::vec::Vec<u32>, u32) {
        (vec![0u32; (size * size) as usize], size)
    }

    #[test]
    fn kind_defaults_to_arrow_and_set_kind_reports_changes() {
        let mut cursor = Cursor::new(Point::new(5, 5));
        assert_eq!(cursor.kind(), CursorKind::Arrow);
        assert!(!cursor.set_kind(CursorKind::Arrow), "no change reported when the kind does not change");
        assert!(cursor.set_kind(CursorKind::Hand));
        assert_eq!(cursor.kind(), CursorKind::Hand);
        assert!(!cursor.set_kind(CursorKind::Hand));
    }

    #[test]
    fn a_kind_with_no_bitmap_of_its_own_ignores_another_kinds() {
        let mut cursor = Cursor::new(Point::new(5, 5));
        assert!(cursor.set_bitmap(CursorKind::Arrow, &[0xFFFF_FFFF; 4], 2, 2, 2, Point::new(0, 0)));
        cursor.set_kind(CursorKind::Move);

        // Arrow's bitmap frame would be 2x2 at this position; Move has no
        // bitmap of its own installed, so it uses its fallback's fixed
        // KIND_SHAPE_SIZE box instead -- switching kinds never borrows
        // another kind's bitmap.
        let frame = cursor.frame();
        assert_eq!(frame.width(), Cursor::KIND_SHAPE_SIZE);
        assert_eq!(frame.height(), Cursor::KIND_SHAPE_SIZE);
    }

    #[test]
    fn each_kind_shows_its_own_installed_bitmap_independently() {
        let mut cursor = Cursor::new(Point::new(5, 5));
        assert!(cursor.set_bitmap(CursorKind::Hand, &[0xFFFF_FFFF; 4], 2, 2, 2, Point::new(1, 1)));

        cursor.set_kind(CursorKind::Hand);
        assert_eq!(cursor.frame(), Rect::new(4, 4, 2, 2), "Hand shows the bitmap installed for Hand");

        // Move has no bitmap installed at all: its own fallback box, not
        // Hand's bitmap and not Arrow's fallback corner-anchoring either.
        cursor.set_kind(CursorKind::Move);
        let frame = cursor.frame();
        assert_eq!(frame.width(), Cursor::KIND_SHAPE_SIZE);
        assert_eq!(frame.height(), Cursor::KIND_SHAPE_SIZE);

        // Back to Hand: still has its own bitmap, unaffected by Move ever having been active.
        cursor.set_kind(CursorKind::Hand);
        assert_eq!(cursor.frame(), Rect::new(4, 4, 2, 2), "switching away and back does not drop the bitmap");
    }

    #[test]
    fn move_fallback_draws_a_spine_through_the_center() {
        let (mut pixels, size) = new_surface(32);
        let mut surface = Surface::new(&mut pixels, size, size).unwrap();
        let center = Point::new(16, 16);
        Cursor::draw_fallback_move(&mut surface, center);

        assert_eq!(surface.pixel(16, 16), Some(0xFFFF_FFFF), "center of the spine is filled");
        assert_eq!(surface.pixel(16, 16 - 7), Some(0xFF00_0000), "the tip of the up arrowhead is black");
        assert_eq!(surface.pixel(0, 0), Some(0), "the surface's far corner, well outside the shape, is untouched");
    }

    #[test]
    fn hand_fallback_draws_a_finger_above_a_wider_palm() {
        let (mut pixels, size) = new_surface(32);
        let mut surface = Surface::new(&mut pixels, size, size).unwrap();
        let center = Point::new(16, 16);
        Cursor::draw_fallback_hand(&mut surface, center);

        assert_eq!(surface.pixel(16, 16 - 7), Some(0xFF00_0000), "the top of the finger is outlined");
        assert_eq!(surface.pixel(16 + 5, 16 + 5), Some(0xFF00_0000), "the palm's right edge is outlined");
        assert_eq!(surface.pixel(16 - 6, 16 + 5), Some(0), "left of the palm, outside the shape, is untouched");
    }

    #[test]
    fn not_allowed_fallback_draws_a_ring_and_a_slash() {
        let (mut pixels, size) = new_surface(32);
        let mut surface = Surface::new(&mut pixels, size, size).unwrap();
        let center = Point::new(16, 16);
        Cursor::draw_fallback_not_allowed(&mut surface, center);

        const RED: u32 = 0xFFE0_3030;
        assert_eq!(surface.pixel(16 + 7, 16), Some(RED), "the ring's right edge is drawn");
        assert_eq!(surface.pixel(16 - 7, 16), Some(RED), "the ring's left edge is drawn");
        assert_eq!(surface.pixel(16, 16), Some(RED), "the slash crosses the center");
        assert_eq!(surface.pixel(16 + 3, 16 + 3), Some(RED), "the slash follows the dx == dy diagonal");
        assert_eq!(surface.pixel(0, 0), Some(0), "the surface's far corner, well outside the shape, is untouched");
    }
}
