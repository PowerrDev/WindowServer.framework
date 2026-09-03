/**
* Copyright (c) 2026 NXU Project. All rights reserved.
*/

/**
* File:        crates/windowserver-nxu/src/ffi.rs
*
* Persistent C ABI bridge between NXU and WindowServer.framework.
*
* NXU owns hardware input and framebuffer presentation.
* WindowServer owns window state, pointer routing, cursor state,
* dragging and software composition.
*/

use core::ptr::NonNull;

use spin::Mutex;

use windowserver::{
    font::BUILTIN_FONT,
    Point,
    PointerButton,
    PointerEvent,
    Rect,
    WindowServer,
};

use crate::{
    Framebuffer,
    platform::{
        FramebufferDescriptor,
        FramebufferInfo,
        PixelFormat,
    },
};

/*
* WindowServer is persistent for the lifetime of the graphical NXU session.
*
* NXU is currently single-core during early graphical bring-up, so access
* occurs through a synchronized runtime container.
*/
#[repr(C)]
pub struct WSDisplay {
    pub framebuffer: *mut u32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

struct WindowServerRuntime {
    server: Option<WindowServer>,
    framebuffer: Option<FramebufferDescriptor>,
}

impl WindowServerRuntime {
    const fn new() -> Self {
        Self {
            server: None,
            framebuffer: None,
        }
    }
}

static WINDOW_SERVER_RUNTIME: Mutex<WindowServerRuntime> =
    Mutex::new(WindowServerRuntime::new());

#[unsafe(no_mangle)]
pub unsafe extern "C" fn windowserver_nxu_init(
    display: WSDisplay,
) -> bool {
    let framebuffer = display.framebuffer;
    let width = display.width;
    let height = display.height;
    let stride = display.stride;

    if framebuffer.is_null()
        || width == 0
        || height == 0
        || stride < width
    {
        return false;
    }

    let Some(address) = NonNull::new(framebuffer) else {
        return false;
    };

    let info = FramebufferInfo::new(
        address,
        width,
        height,
        stride,
        PixelFormat::Argb8888,
    );

    /*
    * Validate framebuffer construction before committing state.
    */
    if Framebuffer::new(info).is_err() {
        return false;
    }

    let mut server = WindowServer::new(
        0xFF1A_1A_1A,
    );

    /*
    * Temporary test scene.
    *
    * This will eventually be replaced by windows created by clients.
    */
    let first = match server.create_window(
        Rect::new(80, 80, 420, 280),
    ) {
        Ok(id) => id,
        Err(_) => return false,
    };

    if !server.fill_window(
        first,
        0xFF2D_6C_BA,
    ) {
        return false;
    }

    let _ = server.draw_text(
        first,
        &BUILTIN_FONT,
        Point::new(24, 50),
        "Welcome to sevOS!",
        0xFFFF_FFFF,
    );

    let second = match server.create_window(
        Rect::new(260, 190, 420, 280),
    ) {
        Ok(id) => id,
        Err(_) => return false,
    };

    if !server.fill_window(
        second,
        0xFFC4_3D_5A,
    ) {
        return false;
    }

    server.invalidate(
        Rect::new(0, 0, width, height),
    );

    /*
    * Start the cursor in the center of the display.
    */
    let cursor_position = Point::new(
        (width / 2) as i32,
        (height / 2) as i32,
    );

    let _ = server.handle_pointer_event(
        PointerEvent::Move {
            position: cursor_position,
        },
    );

    /*
    * Commit persistent state.
    *
    * Store only the pointer-free descriptor inside the synchronized
    * global runtime. The actual NonNull framebuffer pointer is
    * reconstructed at the presentation boundary.
    */
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();

    runtime.server = Some(server);
    runtime.framebuffer = Some(info.descriptor());

    true
}

#[unsafe(no_mangle)]
pub extern "C" fn windowserver_nxu_pointer_move(
    x: i32,
    y: i32,
) -> bool {
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();

    let Some(server) = runtime.server.as_mut() else {
        return false;
    };

    let _ = server.handle_pointer_event(
        PointerEvent::Move {
            position: Point::new(x, y),
        },
    );

    true
}

#[unsafe(no_mangle)]
pub extern "C" fn windowserver_nxu_pointer_button(
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

    let mut runtime = WINDOW_SERVER_RUNTIME.lock();

    let Some(server) = runtime.server.as_mut() else {
        return false;
    };

    let event = if pressed {
        PointerEvent::ButtonDown {
            position: Point::new(x, y),
            button,
        }
    } else {
        PointerEvent::ButtonUp {
            position: Point::new(x, y),
            button,
        }
    };

    let _ = server.handle_pointer_event(event);

    true
}

#[unsafe(no_mangle)]
pub extern "C" fn windowserver_nxu_present() -> bool {
    let mut runtime = WINDOW_SERVER_RUNTIME.lock();

    /*
    * Copy framebuffer metadata before borrowing the WindowServer mutably.
    *
    * FramebufferDescriptor is Copy and contains no raw pointer.
    */
    let Some(descriptor) = runtime.framebuffer else {
        return false;
    };

    /*
    * Reconstruct the actual framebuffer mapping only at the platform
    * presentation boundary.
    */
    let Some(info) = descriptor.framebuffer_info() else {
        return false;
    };

    let Some(server) = runtime.server.as_mut() else {
        return false;
    };

    let mut framebuffer = match Framebuffer::new(info) {
        Ok(framebuffer) => framebuffer,
        Err(_) => return false,
    };

    /*
    * Construct a temporary WindowServer Surface over the NXU-owned
    * framebuffer and compose pending damage.
    */
    match framebuffer.with_surface(
        |surface| server.present(surface),
    ) {
        Ok(Ok(presented)) => presented,
        _ => false,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn windowserver_nxu_cursor_x() -> i32 {
    WINDOW_SERVER_RUNTIME
        .lock()
        .server
        .as_ref()
        .map(|server| server.cursor_position().x)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn windowserver_nxu_cursor_y() -> i32 {
    WINDOW_SERVER_RUNTIME
        .lock()
        .server
        .as_ref()
        .map(|server| server.cursor_position().y)
        .unwrap_or(0)
}
