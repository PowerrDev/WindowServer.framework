/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */

/**
 * File:        crates/windowserver-nxu/src/ffi.rs
 *
 * C ABI bridge between NXU/Aqua and WindowServer.framework.
 *
 * WindowServer owns window management and composition. Aqua/UIService owns
 * window contents and renders client surfaces which are submitted here.
 */

use core::ptr::NonNull;
use core::slice;
use core::ffi::c_void;

use spin::Mutex;

use windowserver::{
    CursorKind,
    Point,
    PointerButton,
    PointerEvent,
    Rect,
    Size,
    WindowFlags,
    WindowServer,
};

use crate::{
    Framebuffer,
    platform::{FramebufferDescriptor, FramebufferInfo, PixelFormat},
};

// NXU's monotonic microsecond counter (already used kernel-side for frame
// pacing -- see UIServicePaceFrame in platform/arm64/services/ui_service.c).
// Declared directly rather than routed through UIService's versioned Host
// ABI: windowserver-nxu already links straight against WindowServer's own
// WS_* C surface (see WSPrivate.h) in the same final kernel image, so this
// is the same tier of direct, same-build coupling as any other WS_* call,
// not a new cross-framework contract.
unsafe extern "C" {
    fn timer_get_microseconds() -> u64;
}

pub type WSDisplayPresentFn = unsafe extern "C" fn(
    context: *mut c_void,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> bool;

#[repr(C)]
pub struct WSDisplay {
    pub framebuffer: *mut u32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub present_context: *mut c_void,
    pub present: Option<WSDisplayPresentFn>,
}

struct WindowServerRuntime {
    server: Option<WindowServer>,
    framebuffer: Option<FramebufferDescriptor>,
    present_context: usize,
    present: Option<WSDisplayPresentFn>,
    last_present_us: u64,
}

impl WindowServerRuntime {
    const fn new() -> Self {
        Self {
            server: None,
            framebuffer: None,
            present_context: 0,
            present: None,
            last_present_us: 0,
        }
    }
}

static WINDOW_SERVER_RUNTIME: Mutex<WindowServerRuntime> =
    Mutex::new(WindowServerRuntime::new());

#[unsafe(export_name = "WS_Initialize")]
pub unsafe extern "C" fn ws_initialize(display: WSDisplay) -> bool {
    let framebuffer = display.framebuffer;
    let width = display.width;
    let height = display.height;
    let stride = display.stride;

    if framebuffer.is_null() || width == 0 || height == 0 || stride < width {
        return false;
    }

    let Some(address) = NonNull::new(framebuffer) else { return false; };

    let info = FramebufferInfo::new(
        address,
        width,
        height,
        stride,
        PixelFormat::Argb8888,
    );

    if Framebuffer::new(info).is_err() {
        return false;
    }

    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    runtime.server = Some(WindowServer::new(0xFF14_151A));
    runtime.framebuffer = Some(info.descriptor());
    runtime.present_context = display.present_context as usize;
    runtime.present = display.present;
    true
}

#[unsafe(export_name = "WS_Create_Window")]
pub extern "C" fn ws_create_window(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    opaque: bool,
    corner_radius: u32,
) -> u32 {
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    let Some(server) = runtime.server.as_mut() else { return 0; };

    let mut flags = WindowFlags::default();
    flags.opaque = opaque;
    flags.corner_radius = corner_radius;

    server
        .create_window_with_flags(Rect::new(x, y, width, height), flags)
        .unwrap_or(0)
}

/// Creates the desktop background layer: always opaque, and flagged so
/// `WindowServer::focus_window` refuses to ever focus or raise it (see that
/// method) -- clicking empty desktop space must not bury the frontmost app
/// window behind it.
#[unsafe(export_name = "WS_Create_Background_Window")]
pub extern "C" fn ws_create_background_window(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> u32 {
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    let Some(server) = runtime.server.as_mut() else { return 0; };

    let mut flags = WindowFlags::default();
    flags.opaque = true;
    flags.is_background = true;

    server
        .create_window_with_flags(Rect::new(x, y, width, height), flags)
        .unwrap_or(0)
}

#[unsafe(export_name = "WS_Destroy_Window")]
pub extern "C" fn ws_destroy_window(id: u32) -> bool {
    if id == 0 { return false; }
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    runtime.server.as_mut().is_some_and(|server| server.destroy_window(id))
}

#[unsafe(export_name = "WS_Move_Window")]
pub extern "C" fn ws_move_window(id: u32, x: i32, y: i32) -> bool {
    if id == 0 { return false; }
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    runtime.server.as_mut().is_some_and(|server| server.move_window(id, Point::new(x, y)))
}

#[unsafe(export_name = "WS_Set_Size_Limits")]
pub extern "C" fn ws_set_size_limits(id: u32, min_width: u32, min_height: u32, max_width: u32, max_height: u32) -> bool {
    if id == 0 { return false; }
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    runtime.server.as_mut().is_some_and(|server| {
        server.set_size_limits(id, Size::new(min_width, min_height), Size::new(max_width, max_height))
    })
}

#[unsafe(export_name = "WS_Resize_Window")]
pub extern "C" fn ws_resize_window(id: u32, x: i32, y: i32, width: u32, height: u32) -> bool {
    if id == 0 { return false; }
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    runtime.server.as_mut().is_some_and(|server| server.resize_window(id, Rect::new(x, y, width, height)))
}

/// The shadow a window casts: 0 none, 1 popup (menus, the Dock, small
/// floating windows), 2 window (the default).
#[unsafe(export_name = "WS_Set_Window_Shadow")]
pub extern "C" fn ws_set_window_shadow(id: u32, style: u32) -> bool {
    if id == 0 { return false; }
    let style = match style {
        0 => windowserver::compositor::ShadowStyle::None,
        1 => windowserver::compositor::ShadowStyle::Popup,
        _ => windowserver::compositor::ShadowStyle::Window,
    };
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    runtime.server.as_mut().is_some_and(|server| server.set_window_shadow(id, style))
}

/// Opaque everywhere except its rounded corners (an app window): WindowServer
/// copies its interior rows whole.
#[unsafe(export_name = "WS_Set_Window_Opaque_Interior")]
pub extern "C" fn ws_set_window_opaque_interior(id: u32, opaque_interior: bool) -> bool {
    if id == 0 { return false; }
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    runtime.server.as_mut().is_some_and(|server| server.set_window_opaque_interior(id, opaque_interior))
}

/// Mark the key window, whose shadow is the darker one.
#[unsafe(export_name = "WS_Set_Window_Key")]
pub extern "C" fn ws_set_window_key(id: u32, key: bool) -> bool {
    if id == 0 { return false; }
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    runtime.server.as_mut().is_some_and(|server| server.set_window_key(id, key))
}

/// The display's density, physical pixels per point as thousandths: shadows
/// are sized in points.
#[unsafe(export_name = "WS_Set_Shadow_Scale")]
pub extern "C" fn ws_set_shadow_scale(permille: u32) {
    windowserver::compositor::set_shadow_scale(permille);
}

#[unsafe(export_name = "WS_Focus_Window")]
pub extern "C" fn ws_focus_window(id: u32) -> bool {
    if id == 0 { return false; }
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    runtime.server.as_mut().is_some_and(|server| server.focus_window(id))
}

#[unsafe(export_name = "WS_Render_Window")]
pub unsafe extern "C" fn ws_render_window(
    id: u32,
    pixels: *const u32,
    width: u32,
    height: u32,
    stride: u32,
) -> bool {
    if id == 0 || pixels.is_null() || width == 0 || height == 0 || stride < width {
        return false;
    }

    let Some(required) = (stride as usize).checked_mul(height as usize) else {
        return false;
    };

    let source = unsafe { slice::from_raw_parts(pixels, required) };

    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    let Some(server) = runtime.server.as_mut() else { return false; };

    // Aqua renders XRGB8888 and uses this keyed pixel for rounded corners.
    server.render_window(
        id,
        source,
        width,
        height,
        stride,
        Some(0x00FF_00FF),
    )
}

/// 0 = Arrow, 1 = Move, 2 = Hand, 3 = NotAllowed, 4 = ResizeHorizontal,
/// 5 = ResizeVertical, 6 = ResizeDiagonalNeSw, 7 = ResizeDiagonalNwSe,
/// 8 = Text (see `CursorKind`). An unrecognized value decodes as Arrow
/// rather than being rejected, so a future kind this build does not know
/// about degrades to the default pointer instead of leaving whatever kind
/// was showing before stuck (`WS_Set_Cursor_Kind`), or failing outright
/// instead of installing a bitmap this build simply has no name for yet
/// (`WS_Set_Cursor`).
fn cursor_kind_from_raw(kind: u32) -> CursorKind {
    match kind {
        1 => CursorKind::Move,
        2 => CursorKind::Hand,
        3 => CursorKind::NotAllowed,
        4 => CursorKind::ResizeHorizontal,
        5 => CursorKind::ResizeVertical,
        6 => CursorKind::ResizeDiagonalNeSw,
        7 => CursorKind::ResizeDiagonalNwSe,
        8 => CursorKind::Text,
        _ => CursorKind::Arrow,
    }
}

#[unsafe(export_name = "WS_Set_Cursor")]
pub unsafe extern "C" fn ws_set_cursor(
    kind: u32,
    pixels: *const u32,
    width: u32,
    height: u32,
    stride: u32,
    hotspot_x: i32,
    hotspot_y: i32,
) -> bool {
    if pixels.is_null() || width == 0 || height == 0 || stride < width {
        return false;
    }

    let Some(required) = (stride as usize).checked_mul(height as usize) else {
        return false;
    };

    let source = unsafe { slice::from_raw_parts(pixels, required) };

    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    let Some(server) = runtime.server.as_mut() else { return false; };
    server.set_cursor_bitmap(cursor_kind_from_raw(kind), source, width, height, stride, Point::new(hotspot_x, hotspot_y))
}

#[unsafe(export_name = "WS_Set_Cursor_Kind")]
pub extern "C" fn ws_set_cursor_kind(kind: u32) -> bool {
    let kind = cursor_kind_from_raw(kind);

    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    let Some(server) = runtime.server.as_mut() else { return false; };
    server.set_cursor_kind(kind);
    true
}

#[unsafe(export_name = "WS_Pointer_Move")]
pub extern "C" fn ws_pointer_move(x: i32, y: i32) -> bool {
    let now_us = unsafe { timer_get_microseconds() };
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    let Some(server) = runtime.server.as_mut() else { return false; };
    let _ = server.handle_pointer_event(PointerEvent::Move { position: Point::new(x, y) }, now_us);
    true
}

#[unsafe(export_name = "WS_Pointer_Button")]
pub extern "C" fn ws_pointer_button(
    x: i32,
    y: i32,
    button: u32,
    pressed: bool,
) -> bool {
    let button = match button {
        0 => PointerButton::Left,
        1 => PointerButton::Right,
        2 => PointerButton::Middle,
        _ => return false,
    };

    let now_us = unsafe { timer_get_microseconds() };
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    let Some(server) = runtime.server.as_mut() else { return false; };
    let event = if pressed {
        PointerEvent::ButtonDown { position: Point::new(x, y), button }
    } else {
        PointerEvent::ButtonUp { position: Point::new(x, y), button }
    };
    let _ = server.handle_pointer_event(event, now_us);
    true
}

#[unsafe(export_name = "WS_Present")]
pub extern "C" fn ws_present() -> bool {
    let now_us = unsafe { timer_get_microseconds() };
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();
    let Some(descriptor) = runtime.framebuffer else { return false; };
    let Some(info) = descriptor.framebuffer_info() else { return false; };
    let Some(server) = runtime.server.as_mut() else { return false; };
    let mut framebuffer = match Framebuffer::new(info) {
        Ok(framebuffer) => framebuffer,
        Err(_) => return false,
    };

    let damage = match framebuffer.with_surface(|surface| server.present(surface, now_us)) {
        Ok(Ok(damage)) => damage,
        _ => return false,
    };

    let Some(damage) = damage else { return false; };

    // No spin-wait here: the host's present callback already paces frames by
    // sleeping (UIServicePaceFrame), and spinning held the boot CPU for up to 16 ms.
    runtime.last_present_us = now_us;

    let Some(present) = runtime.present else { return false; };
    unsafe {
        present(
            runtime.present_context as *mut c_void,
            damage.x() as u32,
            damage.y() as u32,
            damage.width(),
            damage.height(),
        )
    }
}
