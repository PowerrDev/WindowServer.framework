#![no_std]

//! Platform-independent WindowServer core for sevOS.
//!
//! Implementation will be introduced incrementally from the old ArmOS
//! WindowServer architecture rather than as a direct C-to-Rust translation.

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
