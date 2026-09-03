/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */

/**
 * File:        crates/windowserver/src/text.rs
 *
 * Bitmap text rendering for WindowServer.
 *
 * This module draws bitmap glyphs onto software surfaces.
 * Glyph pixels are treated as a monochrome mask: set bits are rendered
 * using the supplied foreground color while unset bits remain untouched.
 */
use crate::{
    font::{BitmapFont, Glyph},
    geometry::Point,
    surface::Surface,
};

// Draw a UTF-8 string using a fixed-size bitmap font.
//
// Characters outside the font's supported range fall back to `?`.
//
// Newlines reset the horizontal position and advance by one glyph height.
pub fn draw_text(
    surface: &mut Surface<'_>,
    font: &BitmapFont,
    text: &str,
    position: Point,
    color: u32,
) {
    let mut cursor_x = position.x;
    let mut cursor_y = position.y;

    for character in text.bytes() {
        match character {
            b'\n' => {
                cursor_x = position.x;
                cursor_y += font.glyph_height() as i32;
            }

            b'\r' => {}

            b'\t' => {
                cursor_x +=
                    (font.glyph_width() * 4) as i32;
            }

            character => {
                if let Some(glyph) =
                    font.glyph_or_fallback(character)
                {
                    draw_glyph(
                        surface,
                        glyph,
                        Point::new(
                            cursor_x,
                            cursor_y,
                        ),
                        color,
                    );
                }

                cursor_x +=
                    font.glyph_width() as i32;
            }
        }
    }
}

// Draw a bitmap glyph onto a surface.
//
// Set glyph pixels are written using `color`.
// Unset glyph pixels leave the destination surface unchanged.
//
// Pixels outside the destination surface are clipped automatically.
pub fn draw_glyph(
    surface: &mut Surface<'_>,
    glyph: Glyph,
    position: Point,
    color: u32,
) {
    for y in 0..glyph.height() {
        for x in 0..glyph.width() {
            if !glyph.pixel(x, y) {
                continue;
            }

            let destination_x =
                position.x + x as i32;

            let destination_y =
                position.y + y as i32;

            if destination_x < 0 || destination_y < 0 {
                continue;
            }

            let destination_x =
                destination_x as u32;

            let destination_y =
                destination_y as u32;

            if destination_x >= surface.width()
                || destination_y >= surface.height()
            {
                continue;
            }

            surface.set_pixel(
                destination_x,
                destination_y,
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /*
     * 4x4 test glyph:
     *
     * 1000
     * 0100
     * 0010
     * 0001
     */
    static TEST_GLYPH_DATA: [u8; 4] = [
        0b1000_0000,
        0b0100_0000,
        0b0010_0000,
        0b0001_0000,
    ];

    #[test]
    fn draws_glyph_pixels() {
        let glyph = Glyph::new(
            &TEST_GLYPH_DATA,
            4,
            4,
            1,
        );

        let mut pixels = [0u32; 64];

        let mut surface =
            Surface::new(
                &mut pixels,
                8,
                8,
            )
            .unwrap();

        draw_glyph(
            &mut surface,
            glyph,
            Point::new(2, 2),
            0xFFFF_FFFF,
        );

        assert_eq!(
            surface.pixel(2, 2),
            Some(0xFFFF_FFFF),
        );

        assert_eq!(
            surface.pixel(3, 3),
            Some(0xFFFF_FFFF),
        );

        assert_eq!(
            surface.pixel(4, 4),
            Some(0xFFFF_FFFF),
        );

        assert_eq!(
            surface.pixel(5, 5),
            Some(0xFFFF_FFFF),
        );

        /*
         * Pixels outside the glyph mask remain untouched.
         */
        assert_eq!(
            surface.pixel(3, 2),
            Some(0),
        );
    }

    #[test]
    fn clips_glyph_at_surface_edge() {
        let glyph = Glyph::new(
            &TEST_GLYPH_DATA,
            4,
            4,
            1,
        );

        let mut pixels = [0u32; 16];

        let mut surface =
            Surface::new(
                &mut pixels,
                4,
                4,
            )
            .unwrap();

        draw_glyph(
            &mut surface,
            glyph,
            Point::new(2, 2),
            0xFFFF_FFFF,
        );

        assert_eq!(
            surface.pixel(2, 2),
            Some(0xFFFF_FFFF),
        );

        assert_eq!(
            surface.pixel(3, 3),
            Some(0xFFFF_FFFF),
        );
    }

    #[test]
    fn clips_negative_positions() {
        let glyph = Glyph::new(
            &TEST_GLYPH_DATA,
            4,
            4,
            1,
        );

        let mut pixels = [0u32; 16];

        let mut surface =
            Surface::new(
                &mut pixels,
                4,
                4,
            )
            .unwrap();

        draw_glyph(
            &mut surface,
            glyph,
            Point::new(-1, -1),
            0xFFFF_FFFF,
        );

        /*
        * The bottom-right portion of the glyph should remain visible.
        */
        assert_eq!(
            surface.pixel(0, 0),
            Some(0xFFFF_FFFF),
        );
    }

    #[test]
    fn draws_multiple_characters() {
        /*
        * Two 4x4 glyphs:
        *
        * A:
        * 1000
        * 0100
        * 0010
        * 0001
        *
        * B:
        * 0001
        * 0010
        * 0100
        * 1000
        */
        static FONT_DATA: [u8; 8] = [
            0b1000_0000,
            0b0100_0000,
            0b0010_0000,
            0b0001_0000,

            0b0001_0000,
            0b0010_0000,
            0b0100_0000,
            0b1000_0000,
        ];

        let font = BitmapFont::new(
            &FONT_DATA,
            4,
            4,
            1,
            4,
            b'A',
            2,
        );

        let mut pixels = [0u32; 128];

        let mut surface =
            Surface::new(
                &mut pixels,
                16,
                8,
            )
            .unwrap();

        draw_text(
            &mut surface,
            &font,
            "AB",
            Point::new(0, 0),
            0xFFFF_FFFF,
        );

        /*
        * First glyph starts at x = 0.
        */
        assert_eq!(
            surface.pixel(0, 0),
            Some(0xFFFF_FFFF),
        );

        /*
        * Second glyph starts at x = 4.
        */
        assert_eq!(
            surface.pixel(7, 0),
            Some(0xFFFF_FFFF),
        );
    }
}