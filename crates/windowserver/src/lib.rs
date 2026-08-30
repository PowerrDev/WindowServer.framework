#![no_std]

extern crate alloc;

/*
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */

/*
 * File:        crates/windowserver/src/lib.rs
 *
 * Platform-independent WindowServer core for NXU
 */

pub mod animation;
pub mod compositor;
pub mod cursor;
pub mod damage;
pub mod decorations;
pub mod geometry;
pub mod input;
pub mod server;
pub mod surface;
pub mod window;

pub use compositor::{Compositor, Layer};
pub use damage::Damage;
pub use geometry::{Point, Rect, Size};
pub use server::WindowServer;
pub use surface::{Surface, SurfaceError};
pub use window::{Window, WindowFlags, WindowId};
