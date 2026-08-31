#![no_std]

/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/**
 * File:        crates/windowserver-nxu/src/lib.rs
 *
 * NXU platform backend for WindowServer.framework.
 */

pub mod ffi;
pub mod framebuffer;
pub mod platform;

pub use framebuffer::{Framebuffer, FramebufferError};
pub use platform::{DisplayPlatform, FramebufferInfo, PixelFormat};

pub use ffi::bootstrap;
