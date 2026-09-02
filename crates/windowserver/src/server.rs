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
use alloc::vec;
use alloc::vec::Vec;

use crate::{
    compositor::{Compositor, Layer},
    cursor::Cursor,
    decorations::{Decorations, WindowHit},
    damage::Damage,
    geometry::{Point, Rect},
    input::{
        PointerButton,
        PointerEvent,
        PointerResult,
        PointerState,
    },
    surface::{Surface, SurfaceError},
    window::{Window, WindowId},
};

struct ServerWindow {
    window: Window,
    pixels: Box<[u32]>,
    stride: u32,
}

/// State for an active window drag operation.
#[derive(Debug, Clone, Copy)]
struct DragState {
    window: WindowId,
    offset_x: i32,
    offset_y: i32,
}

/// Platform-independent WindowServer core.
///
/// Windows are stored in back-to-front order.
/// The last window is therefore frontmost.
pub struct WindowServer {
    compositor: Compositor,
    windows: Vec<ServerWindow>,
    damage: Damage,
    next_window_id: WindowId,

    focused_window: Option<WindowId>,

    pointer: PointerState,
    cursor: Cursor,

    drag: Option<DragState>,
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

            drag: None,
        }
    }

    pub fn compositor(&self) -> &Compositor {
        &self.compositor
    }

    pub fn compositor_mut(&mut self) -> &mut Compositor {
        &mut self.compositor
    }

    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    pub fn damage(&self) -> &Damage {
        &self.damage
    }

    pub fn focused_window(&self) -> Option<WindowId> {
        self.focused_window
    }

    pub fn pointer_position(&self) -> Point {
        self.pointer.position()
    }

    pub fn cursor_position(&self) -> Point {
        self.cursor.position()
    }

    pub fn pressed_button(&self) -> Option<PointerButton> {
        self.pointer.pressed_button()
    }

    pub fn captured_window(&self) -> Option<WindowId> {
        self.pointer.captured_window()
    }

    pub fn dragging_window(&self) -> Option<WindowId> {
        self.drag.map(|drag| drag.window)
    }

    pub fn create_window(
        &mut self,
        frame: Rect,
    ) -> Result<WindowId, SurfaceError> {
        let pixels = pixel_len(
            frame.width(),
            frame.height(),
        )?;

        let id = self.allocate_window_id();

        self.windows.push(ServerWindow {
            window: Window::new(id, frame),
            pixels: vec![0; pixels].into_boxed_slice(),
            stride: frame.width(),
        });

        self.damage.add(frame);

        Ok(id)
    }

    pub fn destroy_window(&mut self, id: WindowId) -> bool {
        let Some(index) = self.window_index(id) else {
            return false;
        };

        let frame = self.windows[index].window.frame();

        self.windows.remove(index);

        if self.focused_window == Some(id) {
            self.focused_window = None;
        }

        if self.pointer.captured_window() == Some(id) {
            self.pointer.release();
        }

        if self.dragging_window() == Some(id) {
            self.drag = None;
        }

        self.damage.add(frame);

        true
    }

    pub fn window(&self, id: WindowId) -> Option<&Window> {
        self.windows
            .get(self.window_index(id)?)
            .map(|entry| &entry.window)
    }

    pub fn window_mut(
        &mut self,
        id: WindowId,
    ) -> Option<&mut Window> {
        let index = self.window_index(id)?;

        Some(&mut self.windows[index].window)
    }

    /// Returns the frontmost window containing the point.
    pub fn hit_test(
        &self,
        point: Point,
    ) -> Option<WindowId> {
        for entry in self.windows.iter().rev() {
            let frame = entry.window.frame();

            let x = point.x;
            let y = point.y;

            if x >= frame.x()
                && y >= frame.y()
                && x < frame.right() as i32
                && y < frame.bottom() as i32
            {
                return Some(entry.window.id());
            }
        }

        None
    }

    pub fn focus_window(
        &mut self,
        id: WindowId,
    ) -> bool {
        if self.window_index(id).is_none() {
            return false;
        }

        let previous = self.focused_window;

        if previous == Some(id) {
            return true;
        }

        if let Some(previous_id) = previous {
            if let Some(window) = self.window(previous_id) {
                self.damage.add(window.frame());
            }
        }

        self.focused_window = Some(id);

        self.bring_to_front(id)
    }

    pub fn move_window(
        &mut self,
        id: WindowId,
        position: Point,
    ) -> bool {
        let Some(index) = self.window_index(id) else {
            return false;
        };

        let window = &mut self.windows[index].window;

        let old_frame = window.frame();

        window.set_position(position);

        let new_frame = window.frame();

        self.damage.add(old_frame);
        self.damage.add(new_frame);

        true
    }

    pub fn bring_to_front(
        &mut self,
        id: WindowId,
    ) -> bool {
        let Some(index) = self.window_index(id) else {
            return false;
        };

        if index + 1 == self.windows.len() {
            return true;
        }

        let frame = self.windows[index].window.frame();

        let entry = self.windows.remove(index);

        self.windows.push(entry);

        self.damage.add(frame);

        true
    }

    pub fn fill_window(
        &mut self,
        id: WindowId,
        pixel: u32,
    ) -> bool {
        let Some(index) = self.window_index(id) else {
            return false;
        };

        let frame = self.windows[index].window.frame();

        self.windows[index].pixels.fill(pixel);

        self.damage.add(frame);

        true
    }

    /// Routes a pointer event through WindowServer.
    pub fn handle_pointer_event(
        &mut self,
        event: PointerEvent,
    ) -> PointerResult {
        match event {
            PointerEvent::Move { position } => {
                self.update_pointer_position(position);

                /*
                 * If a window is actively being dragged, move it.
                 */
                if let Some(drag) = self.drag {
                    let new_position = Point::new(
                        position.x - drag.offset_x,
                        position.y - drag.offset_y,
                    );

                    self.move_window(
                        drag.window,
                        new_position,
                    );
                }

                /*
                 * Captured windows continue receiving pointer events even
                 * when the pointer moves outside their frame.
                 */
                let target = self
                    .pointer
                    .captured_window()
                    .or_else(|| self.hit_test(position));

                PointerResult {
                    position,
                    target,
                    focused: self.focused_window,
                    button: self.pointer.pressed_button(),
                    pressed: self.pointer.pressed_button().is_some(),
                }
            }

            PointerEvent::ButtonDown {
                position,
                button,
            } => {
                self.update_pointer_position(position);

                let target = self.hit_test(position);

                /*
                 * Clicking a window focuses it and brings it forward.
                 */
                if let Some(window) = target {
                    self.focus_window(window);
                }

                self.pointer.press(
                    button,
                    target,
                );

                /*
                 * Begin a drag operation for left-clicks.
                 *
                 * offset_x/y prevents the window from snapping its
                 * top-left corner directly to the cursor.
                 */
                if button == PointerButton::Left {
                    if let Some(window_id) = target {
                        match self.window(window_id).map(|window| Decorations::hit_test(window, position)) {
                            Some(WindowHit::CloseButton) => {
                                self.destroy_window(window_id);
                                self.pointer.release();
                            }
                            Some(WindowHit::TitleBar) => {
                                if let Some(window) = self.window(window_id) {
                                    let frame = window.frame();
                                    self.drag = Some(DragState {
                                        window: window_id,
                                        offset_x: position.x - frame.x(),
                                        offset_y: position.y - frame.y(),
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }

                PointerResult {
                    position,
                    target,
                    focused: self.focused_window,
                    button: Some(button),
                    pressed: true,
                }
            }

            PointerEvent::ButtonUp {
                position,
                button,
            } => {
                self.update_pointer_position(position);

                /*
                 * Get the captured target before releasing it.
                 */
                let target = self
                    .pointer
                    .captured_window()
                    .or_else(|| self.hit_test(position));

                self.pointer.release();

                if button == PointerButton::Left {
                    self.drag = None;
                }

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

    /// Composites damaged windows and cursor into the framebuffer.
    pub fn present(
        &mut self,
        framebuffer: &mut Surface<'_>,
    ) -> Result<bool, SurfaceError> {
        let Some(damage) = self.damage.take() else {
            return Ok(false);
        };

        self.compositor.begin(
            framebuffer,
            damage,
        );

        for entry in &mut self.windows {
            let ServerWindow {
                window,
                pixels,
                stride,
            } = entry;

            let surface = Surface::with_stride(
                pixels,
                window.frame().width(),
                window.frame().height(),
                *stride,
            )?;

            let layer = Layer::new(
                window,
                &surface,
            );

            self.compositor.compose_layer(
                framebuffer,
                &layer,
                damage,
            );
        }

        // Decorations are server-owned overlays above client content.
        for entry in &self.windows {
            Decorations::draw(
                &entry.window,
                framebuffer,
                self.focused_window == Some(entry.window.id()),
            );
        }

        // Cursor is always rendered last.
        self.cursor.draw(framebuffer);

        Ok(true)
    }

    fn update_pointer_position(
        &mut self,
        position: Point,
    ) {
        let old_cursor_frame = self.cursor.frame();

        self.pointer.set_position(position);
        self.cursor.set_position(position);

        let new_cursor_frame = self.cursor.frame();

        /*
         * Damage both old and new cursor positions.
         */
        self.damage.add(old_cursor_frame);
        self.damage.add(new_cursor_frame);
    }

    fn window_index(
        &self,
        id: WindowId,
    ) -> Option<usize> {
        self.windows
            .iter()
            .position(|entry| {
                entry.window.id() == id
            })
    }

    fn allocate_window_id(
        &mut self,
    ) -> WindowId {
        let id = self.next_window_id;

        self.next_window_id =
            self.next_window_id.wrapping_add(1);

        if self.next_window_id == 0 {
            self.next_window_id = 1;
        }

        id
    }

        
    /// Mark a region for recomposition.
    ///
    /// This is primarily used by platform backends when a new scanout surface
    /// becomes active and the entire scene must be composed from scratch.
    pub fn invalidate(
        &mut self,
        rect: Rect,
    ) {
        self.damage.add(rect);
    }
}

fn pixel_len(
    width: u32,
    height: u32,
) -> Result<usize, SurfaceError> {
    if width == 0 || height == 0 {
        return Err(SurfaceError::InvalidDimensions);
    }

    (width as usize)
        .checked_mul(height as usize)
        .ok_or(SurfaceError::InvalidDimensions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_window_composes_over_back_window() {
        let mut server = WindowServer::new(0xFF00_0000);

        let back = server
            .create_window(Rect::new(1, 1, 4, 4))
            .unwrap();

        let front = server
            .create_window(Rect::new(3, 3, 4, 4))
            .unwrap();

        server.fill_window(back, 0xFFFF_0000);
        server.fill_window(front, 0xFF00_FF00);

        let mut pixels = [0u32; 64];

        let mut framebuffer =
            Surface::new(&mut pixels, 8, 8).unwrap();

        assert!(server.present(&mut framebuffer).unwrap());

        assert_eq!(
            framebuffer.pixel(2, 2),
            Some(0xFFFF_0000)
        );

        assert_eq!(
            framebuffer.pixel(3, 3),
            Some(0xFF00_FF00)
        );
    }

    #[test]
    fn moving_window_damages_old_and_new_frames() {
        let mut server = WindowServer::new(0xFF00_0000);

        let id = server
            .create_window(Rect::new(1, 1, 2, 2))
            .unwrap();

        let _ = server.damage.take();

        assert!(server.move_window(
            id,
            Point::new(5, 1),
        ));

        assert_eq!(
            server.damage.rect(),
            Some(Rect::new(1, 1, 6, 2))
        );
    }

    #[test]
    fn bring_to_front_changes_z_order() {
        let mut server = WindowServer::new(0xFF00_0000);

        let first = server
            .create_window(Rect::new(1, 1, 4, 4))
            .unwrap();

        let second = server
            .create_window(Rect::new(1, 1, 4, 4))
            .unwrap();

        server.fill_window(first, 0xFFFF_0000);
        server.fill_window(second, 0xFF00_FF00);

        assert!(server.bring_to_front(first));

        let mut pixels = [0u32; 64];

        let mut framebuffer =
            Surface::new(&mut pixels, 8, 8).unwrap();

        server.present(&mut framebuffer).unwrap();

        assert_eq!(
            framebuffer.pixel(2, 2),
            Some(0xFFFF_0000)
        );
    }

    #[test]
    fn hit_test_returns_frontmost_window() {
        let mut server = WindowServer::new(0xFF00_0000);

        let back = server
            .create_window(Rect::new(1, 1, 6, 6))
            .unwrap();

        let front = server
            .create_window(Rect::new(3, 3, 6, 6))
            .unwrap();

        assert_eq!(
            server.hit_test(Point::new(2, 2)),
            Some(back)
        );

        assert_eq!(
            server.hit_test(Point::new(4, 4)),
            Some(front)
        );

        assert_eq!(
            server.hit_test(Point::new(20, 20)),
            None
        );
    }

    #[test]
    fn focus_window_changes_focus_and_z_order() {
        let mut server = WindowServer::new(0xFF00_0000);

        let first = server
            .create_window(Rect::new(1, 1, 4, 4))
            .unwrap();

        let second = server
            .create_window(Rect::new(1, 1, 4, 4))
            .unwrap();

        assert_eq!(server.focused_window(), None);

        assert!(server.focus_window(first));

        assert_eq!(
            server.focused_window(),
            Some(first)
        );

        assert!(server.focus_window(second));

        assert_eq!(
            server.focused_window(),
            Some(second)
        );
    }

    #[test]
    fn pointer_move_updates_position_and_target() {
        let mut server = WindowServer::new(0xFF00_0000);

        let window = server
            .create_window(Rect::new(10, 10, 20, 20))
            .unwrap();

        let result = server.handle_pointer_event(
            PointerEvent::Move {
                position: Point::new(15, 15),
            },
        );

        assert_eq!(
            result.position,
            Point::new(15, 15)
        );

        assert_eq!(
            result.target,
            Some(window)
        );

        assert_eq!(
            server.pointer_position(),
            Point::new(15, 15)
        );

        assert_eq!(
            server.cursor_position(),
            Point::new(15, 15)
        );
    }

    #[test]
    fn pointer_button_down_focuses_frontmost_window() {
        let mut server = WindowServer::new(0xFF00_0000);

        let back = server
            .create_window(Rect::new(10, 10, 30, 30))
            .unwrap();

        let front = server
            .create_window(Rect::new(20, 20, 30, 30))
            .unwrap();

        let result = server.handle_pointer_event(
            PointerEvent::ButtonDown {
                position: Point::new(25, 25),
                button: PointerButton::Left,
            },
        );

        assert_eq!(result.target, Some(front));
        assert_eq!(result.focused, Some(front));

        assert_eq!(
            server.focused_window(),
            Some(front)
        );

        assert_eq!(
            server.pressed_button(),
            Some(PointerButton::Left)
        );

        assert_eq!(
            server.captured_window(),
            Some(front)
        );

        assert_eq!(
            server.dragging_window(),
            Some(front)
        );

        let _ = back;
    }

    #[test]
    fn pointer_button_up_clears_button_state() {
        let mut server = WindowServer::new(0xFF00_0000);

        let window = server
            .create_window(Rect::new(10, 10, 30, 30))
            .unwrap();

        server.handle_pointer_event(
            PointerEvent::ButtonDown {
                position: Point::new(15, 15),
                button: PointerButton::Left,
            },
        );

        let result = server.handle_pointer_event(
            PointerEvent::ButtonUp {
                position: Point::new(15, 15),
                button: PointerButton::Left,
            },
        );

        assert_eq!(result.target, Some(window));
        assert!(!result.pressed);

        assert_eq!(
            result.button,
            Some(PointerButton::Left)
        );

        assert_eq!(
            server.pressed_button(),
            None
        );

        assert_eq!(
            server.captured_window(),
            None
        );

        assert_eq!(
            server.dragging_window(),
            None
        );
    }

    #[test]
    fn dragging_window_moves_it_using_cursor_offset() {
        let mut server = WindowServer::new(0xFF00_0000);

        let window = server
            .create_window(Rect::new(100, 100, 200, 100))
            .unwrap();

        /*
         * Click 20 pixels from the left edge
         * and 30 pixels from the top edge.
         */
        server.handle_pointer_event(
            PointerEvent::ButtonDown {
                position: Point::new(120, 130),
                button: PointerButton::Left,
            },
        );

        assert_eq!(
            server.dragging_window(),
            Some(window)
        );

        /*
         * Move cursor from (120, 130) to (200, 200).
         *
         * Window origin should move from (100, 100)
         * to (180, 170).
         */
        server.handle_pointer_event(
            PointerEvent::Move {
                position: Point::new(200, 200),
            },
        );

        let frame = server
            .window(window)
            .unwrap()
            .frame();

        assert_eq!(frame.x(), 180);
        assert_eq!(frame.y(), 170);
    }

    #[test]
    fn releasing_pointer_stops_window_drag() {
        let mut server = WindowServer::new(0xFF00_0000);

        let window = server
            .create_window(Rect::new(10, 10, 100, 100))
            .unwrap();

        server.handle_pointer_event(
            PointerEvent::ButtonDown {
                position: Point::new(20, 20),
                button: PointerButton::Left,
            },
        );

        server.handle_pointer_event(
            PointerEvent::Move {
                position: Point::new(100, 100),
            },
        );

        let frame_before_release = server
            .window(window)
            .unwrap()
            .frame();

        server.handle_pointer_event(
            PointerEvent::ButtonUp {
                position: Point::new(100, 100),
                button: PointerButton::Left,
            },
        );

        server.handle_pointer_event(
            PointerEvent::Move {
                position: Point::new(200, 200),
            },
        );

        let frame_after_release = server
            .window(window)
            .unwrap()
            .frame();

        assert_eq!(
            frame_before_release,
            frame_after_release
        );
    }
}
