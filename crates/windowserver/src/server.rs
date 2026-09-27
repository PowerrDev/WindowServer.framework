/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */

/**
 * File:        crates/windowserver/src/server.rs
 *
 * WindowServer state management.
 *
 * The server owns window lifecycle, backing storage, Z-order, damage,
 * pointer input, cursor state and basic window dragging.
 *
 * Platform-specific framebuffer presentation remains outside this crate.
 */

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::{
    compositor::{shadow_damage_rect, Compositor, Layer},
    cursor::{Cursor, CursorKind},
    damage::Damage,
    geometry::{Point, Rect, Size},
    input::{PointerButton, PointerEvent, PointerResult, PointerState, ShakeDetector},
    surface::{Surface, SurfaceError},
    window::{Window, WindowFlags, WindowId},
};

/// What damaging `frame` (a window appearing, disappearing, moving, resizing
/// or changing stacking order) must actually cover: the frame itself, plus
/// its shadow's reach (`compositor::shadow_damage_rect`) for anything but the
/// desktop background, which casts none. Missing this lets a window's old
/// shadow (or the gap where its new one should be) sit un-redrawn outside the
/// frame-only rect -- see the `moving_a_window_recomposites_old_and_new_footprint`
/// test, which caught exactly that.
fn frame_damage_rect(frame: Rect, is_background: bool) -> Rect {
    if is_background { frame } else { shadow_damage_rect(frame) }
}

struct ServerWindow {
    window: Window,
    pixels: Box<[u32]>,
    stride: u32,
    /// The frame this window was last known-correct in the framebuffer at,
    /// set the first time it moves after a present and consumed by
    /// `apply_pending_scrolls` on the next one. `None` means nothing is
    /// pending. See that method for the full reasoning.
    pending_scroll_origin: Option<Rect>,
    /// Its shadow, precomputed (see `compositor::ShadowMask`).
    shadow: Option<crate::compositor::ShadowMask>,
}

/// Platform-independent WindowServer core.
///
/// WindowServer owns window lifecycle, surface storage, ordering, focus,
/// pointer state, damage tracking and composition. Aqua/UIService owns the
/// visual contents of each window.
pub struct WindowServer {
    compositor: Compositor,
    windows: Vec<ServerWindow>,
    damage: Damage,
    next_window_id: WindowId,
    focused_window: Option<WindowId>,
    pointer: PointerState,
    cursor: Cursor,
    shake: ShakeDetector,
}

impl WindowServer {
    pub fn new(background: u32) -> Self {
        let initial_position = Point::new(0, 0);

        Self {
            compositor: Compositor::new(background),
            windows: Vec::new(),
            damage: Damage::new(),
            next_window_id: 1,
            focused_window: None,
            pointer: PointerState::new(initial_position),
            cursor: Cursor::new(initial_position),
            shake: ShakeDetector::new(),
        }
    }

    pub fn compositor(&self) -> &Compositor { &self.compositor }
    pub fn compositor_mut(&mut self) -> &mut Compositor { &mut self.compositor }
    pub fn window_count(&self) -> usize { self.windows.len() }
    pub fn damage(&self) -> &Damage { &self.damage }
    pub fn focused_window(&self) -> Option<WindowId> { self.focused_window }
    pub fn pointer_position(&self) -> Point { self.pointer.position() }
    pub fn cursor_position(&self) -> Point { self.cursor.position() }
    pub fn pressed_button(&self) -> Option<PointerButton> { self.pointer.pressed_button() }
    pub fn captured_window(&self) -> Option<WindowId> { self.pointer.captured_window() }

    /// Install a real cursor bitmap (e.g. a decoded `.cur` asset) for `kind`,
    /// in place of its own built-in placeholder shape whenever `kind` is
    /// active -- every other kind keeps whatever was installed for it.
    pub fn set_cursor_bitmap(
        &mut self,
        kind: CursorKind,
        pixels: &[u32],
        width: u32,
        height: u32,
        stride: u32,
        hotspot: Point,
    ) -> bool {
        let old_frame = self.cursor.frame();
        if !self.cursor.set_bitmap(kind, pixels, width, height, stride, hotspot) {
            return false;
        }
        self.damage.add(old_frame);
        self.damage.add(self.cursor.frame());
        true
    }

    pub const fn cursor_kind(&self) -> CursorKind { self.cursor.kind() }

    /// Switch what the pointer shows (`WS_Set_Cursor_Kind`): WindowServer
    /// itself drives this for window chrome it manages directly, Aqua/
    /// UIService for anything inside a window's content it hit-tests itself
    /// -- see `CursorKind`. Returns `false` (no damage added) if `kind` was
    /// already current.
    pub fn set_cursor_kind(&mut self, kind: CursorKind) -> bool {
        let old_frame = self.cursor.frame();
        if !self.cursor.set_kind(kind) {
            return false;
        }
        self.damage.add(old_frame);
        self.damage.add(self.cursor.frame());
        true
    }

    pub fn create_window(&mut self, frame: Rect) -> Result<WindowId, SurfaceError> {
        self.create_window_with_flags(frame, WindowFlags::default())
    }

    pub fn create_window_with_flags(
        &mut self,
        frame: Rect,
        flags: WindowFlags,
    ) -> Result<WindowId, SurfaceError> {
        let pixels = pixel_len(frame.width(), frame.height())?;
        // Before the id: a window that cannot be backed is not created at all.
        let pixels = zeroed_pixels(pixels).ok_or(SurfaceError::OutOfMemory)?;
        let id = self.allocate_window_id();

        self.windows.push(ServerWindow {
            window: {
                let mut window = Window::new(id, frame);
                window.set_flags(flags);
                window
            },
            pixels,
            stride: frame.width(),
            pending_scroll_origin: None,
            shadow: None,
        });

        self.damage.add(frame_damage_rect(frame, flags.is_background));
        Ok(id)
    }

    /// Set how far a window can be resized. Takes effect on the next
    /// `resize_window` call (or drag, once the client-side gesture is wired
    /// up to it); does not itself resize an already out-of-range window.
    pub fn set_size_limits(&mut self, id: WindowId, min_size: Size, max_size: Size) -> bool {
        let Some(window) = self.window_mut(id) else { return false; };
        window.set_min_size(min_size);
        window.set_max_size(Some(max_size));
        true
    }

    pub fn destroy_window(&mut self, id: WindowId) -> bool {
        let Some(index) = self.window_index(id) else { return false; };
        let window = &self.windows[index].window;
        let damage = frame_damage_rect(window.frame(), window.flags().is_background);
        self.windows.remove(index);

        if self.focused_window == Some(id) {
            self.focused_window = None;
        }
        if self.pointer.captured_window() == Some(id) {
            self.pointer.release();
        }
        self.damage.add(damage);
        true
    }

    pub fn window(&self, id: WindowId) -> Option<&Window> {
        self.windows.get(self.window_index(id)?).map(|entry| &entry.window)
    }

    pub fn window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        let index = self.window_index(id)?;
        Some(&mut self.windows[index].window)
    }

    /// Submit a complete XRGB8888 client surface for an existing window.
    ///
    /// `transparent_key`, when present, leaves matching pixels transparent;
    /// all other pixels are normalized to the WindowServer's ARGB8888 storage.
    pub fn render_window(
        &mut self,
        id: WindowId,
        source: &[u32],
        width: u32,
        height: u32,
        stride: u32,
        transparent_key: Option<u32>,
    ) -> bool {
        let Some(index) = self.window_index(id) else { return false; };
        if width == 0 || height == 0 || stride < width {
            return false;
        }
        if width != self.windows[index].window.frame().width()
            || height != self.windows[index].window.frame().height()
        {
            return false;
        }

        let Some(required) = (stride as usize).checked_mul(height as usize) else {
            return false;
        };
        if source.len() < required {
            return false;
        }

        let entry = &mut self.windows[index];
        let width = width as usize;

        // The bounding box (inclusive columns and rows) of the pixels that
        // actually differ from what this window already held. Recompositing
        // only that, instead of the whole window, keeps a small change (a
        // hover highlight, one selected row) cheap: compositing is most of a
        // redraw's cost and it scales with the damaged area.
        let (mut min_x, mut max_x) = (usize::MAX, 0usize);
        let (mut min_y, mut max_y) = (usize::MAX, 0usize);

        for row in 0..height as usize {
            let source_start = row * stride as usize;
            let destination_start = row * entry.stride as usize;
            let source_row = &source[source_start..source_start + width];
            let destination_row = &mut entry.pixels[destination_start..destination_start + width];

            let (mut first, mut last) = (usize::MAX, 0usize);

            // The key test is hoisted out of the per-pixel loop: this is the
            // hottest loop in a redraw under software emulation.
            match transparent_key {
                Some(key) => {
                    for (column, (destination, &pixel)) in destination_row.iter_mut().zip(source_row).enumerate() {
                        let normalized = if pixel == key { pixel } else { 0xFF00_0000 | (pixel & 0x00FF_FFFF) };
                        if *destination != normalized {
                            *destination = normalized;
                            if first == usize::MAX { first = column; }
                            last = column;
                        }
                    }
                }
                None => {
                    for (column, (destination, &pixel)) in destination_row.iter_mut().zip(source_row).enumerate() {
                        let normalized = 0xFF00_0000 | (pixel & 0x00FF_FFFF);
                        if *destination != normalized {
                            *destination = normalized;
                            if first == usize::MAX { first = column; }
                            last = column;
                        }
                    }
                }
            }

            if first != usize::MAX {
                min_x = min_x.min(first);
                max_x = max_x.max(last);
                min_y = min_y.min(row);
                max_y = row;
            }
        }

        let frame = entry.window.frame();

        // Fresh content invalidates the "just shift the old on-screen
        // pixels" assumption a pending scroll relies on -- see
        // `apply_pending_scrolls`. Fall back to damaging the old frame too,
        // and the whole new one, so a window that both moved and redrew
        // content this cycle still gets both spots right.
        if let Some(original_frame) = entry.pending_scroll_origin.take() {
            self.damage.add(original_frame);
            self.damage.add(frame);
        } else if min_y != usize::MAX {
            self.damage.add(Rect::new(
                frame.x().saturating_add(min_x as i32),
                frame.y().saturating_add(min_y as i32),
                (max_x - min_x + 1) as u32,
                (max_y - min_y + 1) as u32,
            ));
        }

        true
    }

    pub fn hit_test(&self, point: Point) -> Option<WindowId> {
        for entry in self.windows.iter().rev() {
            if !entry.window.flags().visible { continue; }
            if entry.window.frame().contains_point(point) {
                return Some(entry.window.id());
            }
        }
        None
    }

    /// Change the shadow a window casts (a menu or the Dock casts a small one).
    pub fn set_window_shadow(&mut self, id: WindowId, shadow: crate::compositor::ShadowStyle) -> bool {
        self.set_window_flags(id, |flags| flags.shadow = shadow)
    }

    /// Opaque except its rounded corners (see `WindowFlags::opaque_interior`).
    pub fn set_window_opaque_interior(&mut self, id: WindowId, opaque_interior: bool) -> bool {
        self.set_window_flags(id, |flags| flags.opaque_interior = opaque_interior)
    }

    /// Mark the key window (its shadow is the darker one).
    pub fn set_window_key(&mut self, id: WindowId, key: bool) -> bool {
        self.set_window_flags(id, |flags| flags.key = key)
    }

    fn set_window_flags(&mut self, id: WindowId, change: impl FnOnce(&mut WindowFlags)) -> bool {
        let Some(index) = self.window_index(id) else { return false; };
        let window = &mut self.windows[index].window;
        let mut flags = window.flags();
        let before = flags;
        change(&mut flags);
        if flags == before {
            return true;
        }
        window.set_flags(flags);
        let frame = window.frame();
        self.damage.add(frame_damage_rect(frame, flags.is_background));
        true
    }

    pub fn focus_window(&mut self, id: WindowId) -> bool {
        let Some(index) = self.window_index(id) else { return false; };
        // Never focus or raise the desktop background layer: it hit-tests
        // like any other window over empty desktop space, and without this,
        // clicking there would call bring_to_front on a full-screen opaque
        // window and bury whatever app was frontmost -- indistinguishable
        // from that app having closed, since nothing brings it back.
        if self.windows[index].window.flags().is_background { return false; }
        let previous = self.focused_window;
        if previous == Some(id) { return true; }
        if let Some(previous_id) = previous {
            if let Some(window) = self.window(previous_id) {
                self.damage.add(frame_damage_rect(window.frame(), window.flags().is_background));
            }
        }
        self.focused_window = Some(id);
        self.bring_to_front(id)
    }

    /// Reposition a window. Does not damage anything directly -- unlike
    /// most mutators here, the actual damage this produces depends on the
    /// *net* movement since the last present, not this one call in
    /// isolation, so it's resolved lazily by `apply_pending_scrolls` when
    /// `present` next runs. See that method.
    pub fn move_window(&mut self, id: WindowId, position: Point) -> bool {
        let Some(index) = self.window_index(id) else { return false; };
        let entry = &mut self.windows[index];
        if entry.pending_scroll_origin.is_none() {
            entry.pending_scroll_origin = Some(entry.window.frame());
        }
        entry.window.set_position(position);
        true
    }

    /// Resize (and, for a left/top-anchored drag, reposition) a window to
    /// `frame`, clamped to whatever `set_size_limits` last gave it. Lays the
    /// window's pixel buffer out for the new size (growing it when it is too
    /// small) and damages both the old and new footprint -- there's no
    /// pending-scroll-style shortcut here, since the buffer itself is a
    /// different shape now. The caller is expected to submit fresh content
    /// via `render_window` right after this returns; until then the buffer
    /// holds stale or blank pixels.
    pub fn resize_window(&mut self, id: WindowId, frame: Rect) -> bool {
        let Some(index) = self.window_index(id) else { return false; };
        let entry = &mut self.windows[index];
        let old_frame = entry.window.frame();

        entry.window.set_frame(frame);
        let new_frame = entry.window.frame();

        let Ok(pixel_count) = pixel_len(new_frame.width(), new_frame.height()) else {
            return false;
        };
        // A live resize calls this for every step of the drag, and the
        // client renders the new size right after: the old storage is kept
        // while it is big enough (its stale pixels are overwritten before
        // the next composite), and a growing window gets some headroom so
        // the next steps fit too -- up to its largest size, never past it.
        // Allocating and zeroing megabytes per step was most of what a
        // resize cost.
        if pixel_count > entry.pixels.len() {
            let largest = entry.window.max_size().and_then(|max| pixel_len(max.width, max.height).ok()).unwrap_or(usize::MAX);
            let wanted = (pixel_count + pixel_count / 4).min(largest).max(pixel_count);
            // Out of memory (the kernel's arena is finite, and every app's
            // windows share it): the window keeps its size rather than the
            // whole desktop stopping on a failed allocation.
            let Some(pixels) = zeroed_pixels(wanted).or_else(|| zeroed_pixels(pixel_count)) else {
                entry.window.set_frame(old_frame);
                return false;
            };
            entry.pixels = pixels;
        }
        entry.stride = new_frame.width();
        entry.pending_scroll_origin = None;

        let is_background = entry.window.flags().is_background;
        self.damage.add(frame_damage_rect(old_frame, is_background));
        self.damage.add(frame_damage_rect(new_frame, is_background));
        true
    }

    pub fn bring_to_front(&mut self, id: WindowId) -> bool {
        let Some(index) = self.window_index(id) else { return false; };
        if index + 1 == self.windows.len() { return true; }
        let window = &self.windows[index].window;
        let damage = frame_damage_rect(window.frame(), window.flags().is_background);
        let entry = self.windows.remove(index);
        self.windows.push(entry);
        self.damage.add(damage);
        true
    }

    /// `now_us`: the caller's monotonic microsecond clock, used only to
    /// timestamp a shake-triggered cursor grow (see `animation.rs`) -- this
    /// crate never reads a clock itself.
    pub fn handle_pointer_event(&mut self, event: PointerEvent, now_us: u64) -> PointerResult {
        match event {
            PointerEvent::Move { position } => {
                self.update_pointer_position(position, now_us);
                PointerResult {
                    position,
                    target: self.pointer.captured_window().or_else(|| self.hit_test(position)),
                    focused: self.focused_window,
                    button: self.pointer.pressed_button(),
                    pressed: self.pointer.pressed_button().is_some(),
                }
            }
            PointerEvent::ButtonDown { position, button } => {
                self.update_pointer_position(position, now_us);
                let target = self.hit_test(position);
                if let Some(id) = target { self.focus_window(id); }
                self.pointer.press(button, target);
                // A drag's own quick back-and-forth (e.g. wiggling a window
                // into place) should not be mistaken for a shake gesture.
                self.shake.clear();
                PointerResult {
                    position,
                    target,
                    focused: self.focused_window,
                    button: Some(button),
                    pressed: true,
                }
            }
            PointerEvent::ButtonUp { position, button } => {
                self.update_pointer_position(position, now_us);
                let target = self.pointer.captured_window().or_else(|| self.hit_test(position));
                self.pointer.release();
                PointerResult {
                    position,
                    target,
                    focused: self.focused_window,
                    button: Some(button),
                    pressed: false,
                }
            }
        }
    }

    /// Composite all damaged layers into `framebuffer`.
    ///
    /// Returns the damage rect that was actually redrawn, so callers can
    /// present only that region to the display device instead of flushing
    /// the entire scanout on every call.
    ///
    /// `now_us`: the caller's monotonic microsecond clock, used to advance
    /// the cursor's grow/shrink animation by real elapsed time regardless of
    /// how often `present` happens to get called (see `animation.rs`).
    pub fn present(&mut self, framebuffer: &mut Surface<'_>, now_us: u64) -> Result<Option<Rect>, SurfaceError> {
        let old_cursor_frame = self.cursor.frame();
        if self.cursor.step_animation(now_us) {
            self.damage.add(old_cursor_frame);
            self.damage.add(self.cursor.frame());
        }

        self.apply_pending_scrolls(framebuffer);

        let rects = self.damage.take();
        if rects.is_empty() {
            return Ok(None);
        }

        let bounds = Rect::new(0, 0, framebuffer.width(), framebuffer.height());
        let mut redrawn: Option<Rect> = None;

        for rect in rects {
            let Some(clip) = rect.intersection(bounds) else { continue; };

            // The desktop's own opaque background layer covers the clip: no
            // need to clear it first.
            let covered = self.windows.first().is_some_and(|entry| {
                let flags = entry.window.flags();
                flags.is_background && flags.visible && flags.opaque && entry.window.frame().intersection(clip) == Some(clip)
            });
            if !covered {
                self.compositor.begin(framebuffer, clip);
            }

            for entry in &mut self.windows {
                entry.shadow = crate::compositor::ShadowMask::refresh(&entry.window, entry.shadow.take());
                let ServerWindow { window, pixels, stride, shadow, .. } = entry;
                let surface = Surface::with_stride(
                    pixels,
                    window.frame().width(),
                    window.frame().height(),
                    *stride,
                )?;
                let layer = Layer::new(window, &surface).with_shadow(shadow.as_ref());
                self.compositor.compose_layer(framebuffer, &layer, clip);
            }

            redrawn = Some(match redrawn {
                Some(union) => union.union(clip),
                None => clip,
            });
        }

        self.cursor.draw(framebuffer);
        Ok(redrawn)
    }

    /// Resolve every window's net movement since the last present.
    ///
    /// This used to have a fast path (see git history / `scroll_pixels`)
    /// that shifted the framebuffer's already-composited pixels directly
    /// instead of recompositing from each window's own source buffer, on
    /// the theory that a pure translation needs no new pixels except the
    /// vacated strip. That is a pure performance optimization -- windows
    /// are never resized on move, so recompositing both the old and new
    /// frame from `entry.pixels` (already the exact mechanism the static,
    /// verified-correct initial render uses) is always correct, just
    /// somewhat more work per drag step. Given a visible rendering defect
    /// traced to dragging and not reproducible in the scroll math's own
    /// unit tests, damaging both frames and letting the normal per-layer
    /// compositor redraw them from source removes an entire, delicate
    /// code path as a suspect rather than trying to patch it blind.
    fn apply_pending_scrolls(&mut self, framebuffer: &mut Surface<'_>) {
        let _ = framebuffer;

        for entry in self.windows.iter_mut() {
            let Some(original_frame) = entry.pending_scroll_origin.take() else { continue; };
            let new_frame = entry.window.frame();
            let is_background = entry.window.flags().is_background;

            self.damage.add(frame_damage_rect(original_frame, is_background));
            self.damage.add(frame_damage_rect(new_frame, is_background));
        }
    }

    pub fn invalidate(&mut self, rect: Rect) { self.damage.add(rect); }

    fn update_pointer_position(&mut self, position: Point, now_us: u64) {
        let old_cursor_frame = self.cursor.frame();
        self.pointer.set_position(position);
        self.cursor.set_position(position);

        // Shaking a window while dragging it (button held) is not a
        // "locate the pointer" gesture, so don't even feed the detector.
        if self.pointer.pressed_button().is_none() {
            if self.shake.record(position) {
                self.cursor.trigger_grow(now_us);
            }
        } else {
            self.shake.clear();
        }

        let new_cursor_frame = self.cursor.frame();
        self.damage.add(old_cursor_frame);
        self.damage.add(new_cursor_frame);
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
    if width == 0 || height == 0 { return Err(SurfaceError::InvalidDimensions); }
    (width as usize).checked_mul(height as usize).ok_or(SurfaceError::InvalidDimensions)
}

/// `len` zeroed pixels, or `None` when there is no memory for them: in the
/// kernel a failed infallible allocation stops everything, and a window that
/// cannot be backed should only fail itself.
fn zeroed_pixels(len: usize) -> Option<Box<[u32]>> {
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(len).ok()?;
    pixels.resize(len, 0);
    Some(pixels.into_boxed_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_window_updates_backing_surface() {
        let mut server = WindowServer::new(0xFF00_0000);
        let id = server.create_window(Rect::new(1, 1, 4, 4)).unwrap();
        let pixels = [0xFF12_3456u32; 16];
        assert!(server.render_window(id, &pixels, 4, 4, 4, None));
        assert!(server.damage().rect().is_some());
    }

    #[test]
    fn moving_a_window_recomposites_old_and_new_footprint() {
        let mut server = WindowServer::new(0xFF11_1111);
        // Out of the way of the window under test -- the cursor (even the
        // built-in fallback shape, at its default (0,0)) draws over
        // whatever's beneath it unconditionally at the end of `present`.
        server.handle_pointer_event(PointerEvent::Move { position: Point::new(50, 50) }, 0);
        let id = server.create_window(Rect::new(0, 0, 4, 4)).unwrap();
        let pixels = [0xFFAA_BBCCu32; 16];
        assert!(server.render_window(id, &pixels, 4, 4, 4, None));

        let mut framebuffer_pixels = [0u32; 8 * 8];
        let mut framebuffer = Surface::new(&mut framebuffer_pixels, 8, 8).unwrap();
        assert!(server.present(&mut framebuffer, 0).unwrap().is_some());

        // Baseline: window fully covers (0,0)-(4,4). One pixel right of it
        // (distance 1 of SHADOW_RIGHT's 9) is inside the shadow's reach, not
        // plain background: blend(0xFF111111, black at alpha 35) = 0xFF0F0F0F.
        assert_eq!(framebuffer.pixel(0, 0), Some(0xFFAA_BBCC));
        assert_eq!(framebuffer.pixel(3, 3), Some(0xFFAA_BBCC));
        assert_eq!(framebuffer.pixel(4, 0), Some(0xFF0F_0F0F));

        assert!(server.move_window(id, Point::new(1, 0)));
        assert!(server.present(&mut framebuffer, 1).unwrap().is_some());

        // Vacated column reverts to background -- except it is now one pixel
        // left of the window's new position (distance 1 of SHADOW_LEFT's 3),
        // so it is the shadow's blend there too, not plain background:
        // blend(0xFF111111, black at alpha 26) = 0xFF0F0F0F (coincidentally
        // the same value as the right-edge blend above -- different alpha,
        // same result once rounded to an 8-bit channel).
        assert_eq!(framebuffer.pixel(0, 0), Some(0xFF0F_0F0F));
        // The whole new footprint is correct, including the freshly-covered
        // rightmost column, and one pixel past *that* is the same
        // right-edge shadow blend the baseline already established above.
        assert_eq!(framebuffer.pixel(1, 0), Some(0xFFAA_BBCC));
        assert_eq!(framebuffer.pixel(4, 0), Some(0xFFAA_BBCC));
        assert_eq!(framebuffer.pixel(5, 0), Some(0xFF0F_0F0F));
    }

    #[test]
    fn moving_a_window_off_and_back_on_screen_stays_correct() {
        // A jump far enough that the old and new footprints don't overlap
        // at all.
        let mut server = WindowServer::new(0xFF11_1111);
        server.handle_pointer_event(PointerEvent::Move { position: Point::new(50, 50) }, 0);
        let id = server.create_window(Rect::new(0, 0, 2, 2)).unwrap();
        let pixels = [0xFFAA_BBCCu32; 4];
        assert!(server.render_window(id, &pixels, 2, 2, 2, None));

        let mut framebuffer_pixels = [0u32; 8 * 8];
        let mut framebuffer = Surface::new(&mut framebuffer_pixels, 8, 8).unwrap();
        assert!(server.present(&mut framebuffer, 0).unwrap().is_some());

        assert!(server.move_window(id, Point::new(6, 6)));
        assert!(server.present(&mut framebuffer, 1).unwrap().is_some());

        assert_eq!(framebuffer.pixel(0, 0), Some(0xFF11_1111));
        assert_eq!(framebuffer.pixel(6, 6), Some(0xFFAA_BBCC));
        assert_eq!(framebuffer.pixel(7, 7), Some(0xFFAA_BBCC));
    }

    #[test]
    fn rendering_the_same_pixels_again_damages_nothing() {
        let mut server = WindowServer::new(0xFF11_1111);
        let id = server.create_window(Rect::new(3, 2, 4, 3)).unwrap();
        let pixels = [0xFF10_2030u32; 12];
        assert!(server.render_window(id, &pixels, 4, 3, 4, None));
        assert!(server.damage().rect().is_some());

        let mut framebuffer_pixels = [0u32; 12 * 8];
        let mut framebuffer = Surface::new(&mut framebuffer_pixels, 12, 8).unwrap();
        assert!(server.present(&mut framebuffer, 0).unwrap().is_some());

        assert!(server.render_window(id, &pixels, 4, 3, 4, None));
        assert!(server.damage().rect().is_none());
    }

    #[test]
    fn only_the_pixels_that_changed_are_damaged() {
        let mut server = WindowServer::new(0xFF11_1111);
        let id = server.create_window(Rect::new(3, 2, 4, 3)).unwrap();
        let mut pixels = [0xFF10_2030u32; 12];
        assert!(server.render_window(id, &pixels, 4, 3, 4, None));

        let mut framebuffer_pixels = [0u32; 12 * 8];
        let mut framebuffer = Surface::new(&mut framebuffer_pixels, 12, 8).unwrap();
        assert!(server.present(&mut framebuffer, 0).unwrap().is_some());

        // One pixel: column 2, row 1 of the window, which sits at (3, 2).
        pixels[4 + 2] = 0xFF99_8877;
        assert!(server.render_window(id, &pixels, 4, 3, 4, None));
        assert_eq!(server.damage().rect(), Some(Rect::new(5, 3, 1, 1)));
        assert!(server.present(&mut framebuffer, 1).unwrap().is_some());
        assert_eq!(framebuffer.pixel(5, 3), Some(0xFF99_8877));

        // Two far-apart pixels: the damage spans both.
        pixels[0] = 0xFF01_0203;
        pixels[11] = 0xFF04_0506;
        assert!(server.render_window(id, &pixels, 4, 3, 4, None));
        assert_eq!(server.damage().rect(), Some(Rect::new(3, 2, 4, 3)));
    }

    #[test]
    fn a_moved_window_that_redraws_is_damaged_in_full() {
        let mut server = WindowServer::new(0xFF11_1111);
        // Park the software cursor off the tiny framebuffer so its sprite does
        // not land on the pixels checked below.
        server.handle_pointer_event(PointerEvent::Move { position: Point::new(50, 50) }, 0);
        let id = server.create_window(Rect::new(0, 0, 2, 2)).unwrap();
        let pixels = [0xFFAA_BBCCu32; 4];
        assert!(server.render_window(id, &pixels, 2, 2, 2, None));

        let mut framebuffer_pixels = [0u32; 8 * 8];
        let mut framebuffer = Surface::new(&mut framebuffer_pixels, 8, 8).unwrap();
        assert!(server.present(&mut framebuffer, 0).unwrap().is_some());

        // Moved, and redrawn with identical pixels in the same cycle: the new
        // spot must still be composited even though no pixel changed. (The
        // software cursor sits at the framebuffer's top-left corner, so the
        // checks stay clear of its sprite, as the move test above does.)
        assert!(server.move_window(id, Point::new(6, 6)));
        assert!(server.render_window(id, &pixels, 2, 2, 2, None));
        assert!(server.present(&mut framebuffer, 1).unwrap().is_some());
        assert_eq!(framebuffer.pixel(6, 6), Some(0xFFAA_BBCC));
        assert_eq!(framebuffer.pixel(7, 7), Some(0xFFAA_BBCC));
        assert_eq!(framebuffer.pixel(0, 0), Some(0xFF11_1111));
    }

    #[test]
    fn transparent_windows_blend_over_background() {
        let mut server = WindowServer::new(0xFF00_0000);
        // The software cursor starts at the framebuffer's top-left and its
        // sprite covers (1, 1): park it off the framebuffer, as the move test
        // does, so the check sees the background and not the cursor.
        server.handle_pointer_event(PointerEvent::Move { position: Point::new(50, 50) }, 0);
        let mut flags = WindowFlags::default();
        flags.opaque = false;
        let id = server.create_window_with_flags(Rect::new(1, 1, 2, 2), flags).unwrap();
        let pixels = [0x00FF_00FFu32, 0x80FF_0000, 0x00FF_00FF, 0x00FF_00FF];
        assert!(server.render_window(id, &pixels, 2, 2, 2, Some(0x00FF_00FF)));

        let mut framebuffer_pixels = [0u32; 16];
        let mut framebuffer = Surface::new(&mut framebuffer_pixels, 4, 4).unwrap();
        assert!(server.present(&mut framebuffer, 0).unwrap().is_some());

        assert_eq!(framebuffer.pixel(1, 1), Some(0xFF00_0000));
        assert_ne!(framebuffer.pixel(2, 1), Some(0xFF00_0000));
    }
}
