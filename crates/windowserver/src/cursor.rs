/*!
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/*! Software cursor primitives. */

use crate::{geometry::{Point, Rect}, surface::Surface};

#[derive(Debug, Clone, Copy)]
pub struct Cursor { position: Point }

impl Cursor {
    pub const WIDTH: u32 = 12;
    pub const HEIGHT: u32 = 18;
    pub const fn new(position: Point) -> Self { Self { position } }
    pub const fn position(&self) -> Point { self.position }
    pub fn set_position(&mut self, position: Point) { self.position = position; }
    pub fn frame(&self) -> Rect { Rect::new(self.position.x, self.position.y, Self::WIDTH, Self::HEIGHT) }

    pub fn draw(&self, surface: &mut Surface<'_>) {
        // Classic arrow, black outline + white fill.
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
                let px = self.position.x + x as i32;
                let py = self.position.y + y as i32;
                if px >= 0 && py >= 0 { surface.set_pixel(px as u32, py as u32, pixel); }
            }
        }
    }
}
