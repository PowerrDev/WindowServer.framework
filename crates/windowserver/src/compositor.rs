/*!
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/*!
 * File:        crates/windowserver/src/compositor.rs
 *
 * Minimal software compositor.
 *
 * This first implementation deliberately has no blur or platform knowledge.
 * It composes window surfaces back-to-front into a target ARGB8888 surface
 * and restricts work to the supplied damage rectangle. It does draw one
 * decoration itself, a drop shadow (see `draw_shadow`): client surfaces are
 * transported to WindowServer color-keyed, not with real per-pixel alpha (see
 * `ui-render`'s `fill_rounded_rect` for why), so a *soft* shadow can only be
 * drawn where real alpha blending is actually available -- here, against
 * whatever is really behind the window, not baked into the client's own
 * surface against a guessed background.
 */

use core::sync::atomic::{AtomicU32, Ordering};

use crate::{geometry::Rect, surface::Surface, window::Window};

/// Drop shadows, after armOS's WindowServer: the window's own rounded
/// rectangle, sunk a little and blurred outward, its falloff a true distance
/// from that shape (quadratic, close to a Gaussian at this size and far
/// cheaper than a blur pass). A window casts a large, soft shadow -- darker
/// for the key window, as on macOS -- and a popup (a menu, the Dock, a name
/// bubble) a small, tight one. Sizes are points, scaled by the display's
/// density (`set_shadow_scale`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ShadowStyle {
    None,
    Popup,
    #[default]
    Window,
}

struct ShadowParams {
    extent: i32,
    offset_y: i32,
    peak: u32,
}

const WINDOW_EXTENT_PT: i32 = 22;
const WINDOW_OFFSET_PT: i32 = 8;
const WINDOW_PEAK_KEY: u32 = 108;
const WINDOW_PEAK_INACTIVE: u32 = 58;
const POPUP_EXTENT_PT: i32 = 9;
const POPUP_OFFSET_PT: i32 = 3;
const POPUP_PEAK: u32 = 64;

static SHADOW_SCALE_PERMILLE: AtomicU32 = AtomicU32::new(1000);

/// Physical pixels per point, as thousandths: shadows are sized in points.
pub fn set_shadow_scale(permille: u32) {
    if permille != 0 {
        SHADOW_SCALE_PERMILLE.store(permille, Ordering::Relaxed);
    }
}

fn pt(points: i32) -> i32 {
    (points as i64 * SHADOW_SCALE_PERMILLE.load(Ordering::Relaxed) as i64 / 1000).max(1) as i32
}

fn shadow_params(style: ShadowStyle, key: bool) -> Option<ShadowParams> {
    match style {
        ShadowStyle::None => None,
        ShadowStyle::Popup => Some(ShadowParams { extent: pt(POPUP_EXTENT_PT), offset_y: pt(POPUP_OFFSET_PT), peak: POPUP_PEAK }),
        ShadowStyle::Window => Some(ShadowParams {
            extent: pt(WINDOW_EXTENT_PT),
            offset_y: pt(WINDOW_OFFSET_PT),
            peak: if key { WINDOW_PEAK_KEY } else { WINDOW_PEAK_INACTIVE },
        }),
    }
}

/// `frame` grown by the largest shadow's reach: what `Damage` needs whenever
/// a window's frame changes or its stacking order does (a shadow can gain or
/// lose whatever is now behind it). The desktop background casts none.
pub fn shadow_damage_rect(frame: Rect) -> Rect {
    let extent = pt(WINDOW_EXTENT_PT);
    let offset = pt(WINDOW_OFFSET_PT);
    Rect::new(
        frame.x() - extent,
        frame.y() - extent + offset.min(extent),
        frame.width() + (extent * 2) as u32,
        frame.height() + (extent * 2) as u32,
    )
}

// A window together with the pixels currently attached to it.
//
// Window lifetime/state ownership remains with `server.rs`; the compositor only
// borrows a snapshot of the layers it must present.
pub struct Layer<'a, 'pixels> {
    pub window: &'a Window,
    pub surface: &'a Surface<'pixels>,
    /// The window's shadow, precomputed (see `ShadowMask`); `None` draws it
    /// from scratch.
    pub shadow: Option<&'a ShadowMask>,
}

impl<'a, 'pixels> Layer<'a, 'pixels> {
    pub const fn new(window: &'a Window, surface: &'a Surface<'pixels>) -> Self {
        Self { window, surface, shadow: None }
    }

    pub const fn with_shadow(mut self, shadow: Option<&'a ShadowMask>) -> Self {
        self.shadow = shadow;
        self
    }
}

/// A window's shadow as an alpha mask over its reach, made once for its
/// size, corner radius, key state and the display's scale, and reused every
/// frame until one of those changes. Working the distances out per pixel
/// every frame was most of what a window drag cost. Zero inside the window's
/// own shape, which its pixels cover.
pub struct ShadowMask {
    key: (u32, u32, u32, ShadowStyle, bool, u32),
    /// Where the mask's top-left sits relative to the window's origin.
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    alpha: alloc::boxed::Box<[u8]>,
}

impl ShadowMask {
    /// The mask for `window` now: `current` again when it still fits.
    pub fn refresh(window: &Window, current: Option<ShadowMask>) -> Option<ShadowMask> {
        let flags = window.flags();
        if flags.is_background {
            return None;
        }
        let frame = window.frame();
        let key = (frame.width(), frame.height(), flags.corner_radius, flags.shadow, flags.key, SHADOW_SCALE_PERMILLE.load(Ordering::Relaxed));
        if let Some(current) = current {
            if current.key == key {
                return Some(current);
            }
        }
        let params = shadow_params(flags.shadow, flags.key)?;
        let (width, height) = (frame.width() as i32, frame.height() as i32);
        if width == 0 || height == 0 {
            return None;
        }
        let radius = (flags.corner_radius as i32).min(width / 2).min(height / 2).max(0);
        let extent = params.extent.clamp(1, 255);
        let table = falloff_table(extent, params.peak);
        let (mask_width, mask_height) = (width + extent * 2, height + extent * 2);
        let (origin_x, origin_y) = (-extent, -extent + params.offset_y);
        // No memory for it: the window goes without a shadow this time.
        let mut alpha = alloc::vec::Vec::new();
        alpha.try_reserve_exact((mask_width * mask_height) as usize).ok()?;
        alpha.resize((mask_width * mask_height) as usize, 0u8);
        let mut alpha = alpha.into_boxed_slice();
        for my in 0..mask_height {
            let y = origin_y + my;
            // Between the corners every column of the window's own width is
            // under the window (nothing to shade): skip straight past it.
            // That is nearly the whole mask for a big window, and this runs
            // again for every step of a live resize.
            let straight = y >= radius && y < height - radius;
            let mut mx = 0;
            while mx < mask_width {
                let x = origin_x + mx;
                if straight && x == 0 {
                    mx += width;
                    continue;
                }
                let index = (my * mask_width + mx) as usize;
                mx += 1;
                if x >= 0 && x < width && y >= 0 && y < height && rounded_distance(x, y, 0, 0, width, height, radius) <= 0 {
                    continue;
                }
                let distance = rounded_distance(x, y, 0, params.offset_y, width, height + params.offset_y, radius);
                alpha[index] =
                    if distance <= 0 { params.peak as u8 } else if distance < extent { table[distance as usize] } else { 0 };
            }
        }
        Some(ShadowMask { key, x: origin_x, y: origin_y, width: mask_width as u32, height: mask_height as u32, alpha })
    }
}

/// Alpha by whole-pixel distance past the shape: quadratic falloff.
fn falloff_table(extent: i32, peak: u32) -> [u8; 256] {
    let mut table = [0u8; 256];
    for (distance, alpha) in table.iter_mut().enumerate().take(extent as usize) {
        let remaining = (extent - distance as i32) as u32;
        *alpha = (peak * remaining * remaining / (extent as u32 * extent as u32)) as u8;
    }
    table
}

/// Black at `alpha` over an opaque pixel: two multiplies, no division (the
/// shadow is always black, so only the destination is scaled).
#[inline(always)]
fn darken(pixel: u32, alpha: u32) -> u32 {
    let keep = 256 - alpha - (alpha >> 7);
    let red_blue = (((pixel & 0x00FF_00FF) * keep + 0x0080_0080) >> 8) & 0x00FF_00FF;
    let green = (((pixel & 0x0000_FF00) * keep + 0x0000_8000) >> 8) & 0x0000_FF00;
    0xFF00_0000 | red_blue | green
}

// Platform-independent software compositor.
pub struct Compositor {
    background: u32,
}

impl Compositor {
    pub const fn new(background: u32) -> Self { Self { background } }
    pub const fn background(&self) -> u32 { self.background }
    pub fn set_background(&mut self, background: u32) { self.background = background; }

    // Compose `layers` in slice order: first is backmost, last is frontmost.
    pub fn compose<'a, 'pixels>(
        &self,
        framebuffer: &mut Surface<'_>,
        layers: &[Layer<'a, 'pixels>],
        damage: Rect,
    ) {
        let Some(clip) = damage.intersection(surface_rect(framebuffer)) else { return; };

        self.begin(framebuffer, clip);

        for layer in layers {
            self.compose_layer(framebuffer, layer, clip);
        }
    }

    // Begin a composition pass by clearing the damaged region.
    pub fn begin(&self, framebuffer: &mut Surface<'_>, damage: Rect) {
        let Some(clip) = damage.intersection(surface_rect(framebuffer)) else { return; };
        fill_rect(framebuffer, clip, self.background);
    }

    // Compose one layer into an already-initialized composition pass.
    pub fn compose_layer<'a, 'pixels>(
        &self,
        framebuffer: &mut Surface<'_>,
        layer: &Layer<'a, 'pixels>,
        clip: Rect,
    ) {
        let frame = layer.window.frame();

        if !layer.window.flags().is_background && layer.window.flags().visible {
            match layer.shadow {
                Some(mask) => draw_shadow_mask(framebuffer, layer.window, mask, clip),
                None => self.draw_shadow(framebuffer, layer.window, clip),
            }
        }

        let Some(area) = frame.intersection(clip) else { return; };

        let source = layer.surface;
        let copy_width = frame.width().min(source.width());
        let copy_height = frame.height().min(source.height());
        let drawable = Rect::new(frame.x(), frame.y(), copy_width, copy_height);
        let Some(area) = drawable.intersection(area) else { return; };

        let flags = layer.window.flags();
        let opaque = flags.opaque;
        // Rows of a shaped window that hold no corner, in frame coordinates.
        // UIService's squircle corners run one and a half radii along each
        // edge (`ui_render::corner_extent`), so that many rows are shaped.
        let corner = if flags.opaque_interior {
            (flags.corner_radius as i32 * 3 / 2).min(frame.height() as i32 / 2)
        } else {
            i32::MAX / 4
        };
        let solid_top = frame.y() + corner;
        let solid_bottom = frame.bottom() as i32 - corner;
        let row_width = area.width() as usize;
        let source_stride = source.stride() as usize;
        let dest_stride = framebuffer.stride() as usize;
        let source_x0 = (area.x() - frame.x()) as usize;

        // Resolve row offsets once per row instead of once per pixel: at
        // window sizes in the hundreds of thousands of pixels (a dragged
        // window recomposites its full frame every pointer-move frame), the
        // per-pixel bounds-checked `pixel()`/`set_pixel()` calls this used to
        // make dominated compositing time, especially under QEMU's software
        // (TCG) CPU emulation. Slice-based row access lets the compiler
        // bounds-check once per row and auto-vectorize the copy/blend loop.
        for y in area.y()..area.bottom() as i32 {
            let source_y = (y - frame.y()) as usize;
            let source_start = source_y * source_stride + source_x0;
            let dest_start = y as usize * dest_stride + area.x() as usize;

            let source_row = &source.pixels()[source_start..source_start + row_width];
            let dest_row = &mut framebuffer.pixels_mut()[dest_start..dest_start + row_width];

            // Opaque rows are a plain copy: the scanout ignores the alpha
            // byte (XRGB), and nothing reads it back.
            if opaque || (y >= solid_top && y < solid_bottom) {
                dest_row.copy_from_slice(source_row);
                continue;
            }

            for (destination, &source_pixel) in dest_row.iter_mut().zip(source_row.iter()) {
                if opaque || source_pixel >> 24 == 0xFF {
                    *destination = 0xFF00_0000 | (source_pixel & 0x00FF_FFFF);
                } else if source_pixel >> 24 != 0 {
                    *destination = blend(*destination, source_pixel);
                }
            }
        }
    }

    // The window's drop shadow, into whatever the framebuffer already holds
    // behind it (the desktop, or windows further back), before the window's
    // own pixels: those cover it where they are opaque, and its see-through
    // rounded corners show the shadow curving around them.
    fn draw_shadow(&self, framebuffer: &mut Surface<'_>, window: &Window, clip: Rect) {
        let flags = window.flags();
        let Some(params) = shadow_params(flags.shadow, flags.key) else { return; };
        let frame = window.frame();
        if frame.width() == 0 || frame.height() == 0 {
            return;
        }
        let radius = (flags.corner_radius as i32).min(frame.width() as i32 / 2).min(frame.height() as i32 / 2).max(0);

        let reach = Rect::new(
            frame.x() - params.extent,
            frame.y() - params.extent + params.offset_y,
            frame.width() + (params.extent * 2) as u32,
            frame.height() + (params.extent * 2) as u32,
        );
        let Some(bounds) = surface_rect(framebuffer).intersection(clip) else { return; };
        let Some(area) = reach.intersection(bounds) else { return; };

        let extent = params.extent.clamp(1, 255);
        let table = falloff_table(extent, params.peak);

        let (left, top) = (frame.x(), frame.y());
        let (right, bottom) = (frame.right() as i32, frame.bottom() as i32);
        let shape_top = top + params.offset_y;
        let shape_bottom = bottom + params.offset_y;
        let stride = framebuffer.stride() as usize;
        let pixels = framebuffer.pixels_mut();

        for y in area.y()..area.bottom() as i32 {
            // Where the window itself covers this row completely (between its
            // rounded corners), the shadow is hidden: skip that span at once.
            let (covered_left, covered_right) = if y >= top + radius && y < bottom - radius {
                (left, right)
            } else if y >= top && y < bottom {
                (left + radius, right - radius)
            } else {
                (right, right)
            };

            let row = y as usize * stride;
            let mut x = area.x();
            while x < area.right() as i32 {
                if x >= covered_left && x < covered_right {
                    x = covered_right;
                    continue;
                }
                // Inside the window's own rounded shape: its pixels decide.
                if x >= left && x < right && y >= top && y < bottom && rounded_distance(x, y, left, top, right, bottom, radius) <= 0 {
                    x += 1;
                    continue;
                }
                let distance = rounded_distance(x, y, left, shape_top, right, shape_bottom, radius);
                let alpha = if distance <= 0 { params.peak } else if distance < extent { table[distance as usize] as u32 } else { 0 };
                if alpha != 0 {
                    let pixel = &mut pixels[row + x as usize];
                    *pixel = darken(*pixel, alpha);
                }
                x += 1;
            }
        }
    }
}

/// A window's shadow from its precomputed mask: the same band as
/// `draw_shadow`, the window's own rows skipped, but only a table read and a
/// darken per pixel.
fn draw_shadow_mask(framebuffer: &mut Surface<'_>, window: &Window, mask: &ShadowMask, clip: Rect) {
    let frame = window.frame();
    let reach = Rect::new(frame.x() + mask.x, frame.y() + mask.y, mask.width, mask.height);
    let Some(bounds) = surface_rect(framebuffer).intersection(clip) else { return; };
    let Some(area) = reach.intersection(bounds) else { return; };

    let radius = (window.flags().corner_radius as i32).min(frame.width() as i32 / 2).min(frame.height() as i32 / 2).max(0);
    let (left, top) = (frame.x(), frame.y());
    let (right, bottom) = (frame.right() as i32, frame.bottom() as i32);
    let stride = framebuffer.stride() as usize;
    let pixels = framebuffer.pixels_mut();
    let mask_width = mask.width as usize;

    for y in area.y()..area.bottom() as i32 {
        let (covered_left, covered_right) = if y >= top + radius && y < bottom - radius {
            (left, right)
        } else {
            (right, right)
        };
        let row = y as usize * stride;
        let mask_row = (y - reach.y()) as usize * mask_width;
        let mut x = area.x();
        while x < area.right() as i32 {
            if x >= covered_left && x < covered_right {
                x = covered_right;
                continue;
            }
            let alpha = mask.alpha[mask_row + (x - reach.x()) as usize] as u32;
            if alpha != 0 {
                let pixel = &mut pixels[row + x as usize];
                *pixel = darken(*pixel, alpha);
            }
            x += 1;
        }
    }
}

/// How far (x, y) is outside the rectangle (left, top)-(right, bottom) with
/// corners of `radius`, in whole pixels; 0 or less inside. Along the straight
/// sides that is a plain difference; only the corners need a square root.
fn rounded_distance(x: i32, y: i32, left: i32, top: i32, right: i32, bottom: i32, radius: i32) -> i32 {
    let inner_left = left + radius;
    let inner_right = right - 1 - radius;
    let inner_top = top + radius;
    let inner_bottom = bottom - 1 - radius;
    let dx = if x < inner_left { inner_left - x } else if x > inner_right { x - inner_right } else { 0 };
    let dy = if y < inner_top { inner_top - y } else if y > inner_bottom { y - inner_bottom } else { 0 };
    let length = if dx == 0 {
        dy
    } else if dy == 0 {
        dx
    } else {
        isqrt((dx as u64 * dx as u64) + (dy as u64 * dy as u64)) as i32
    };
    length - radius
}

// Integer floor(sqrt(value)), no floats: this target has no hardware FPU
// (aarch64-unknown-none-softfloat).
//
// Digit by digit, two bits of `value` per step: shifts, adds and compares
// only. It used to be Newton's method, a 64-bit division per step, which
// i386 has no instruction for (every one a libgcc call): with a shadow mask
// rebuilt for every step of a live resize, that was most of a present there.
fn isqrt(value: u64) -> u64 {
    let mut remainder = value;
    let mut root = 0u64;
    let mut bit = 1u64 << 62;
    while bit > remainder {
        bit >>= 2;
    }
    while bit != 0 {
        if remainder >= root + bit {
            remainder -= root + bit;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    root
}

fn surface_rect(surface: &Surface<'_>) -> Rect {
    Rect::new(0, 0, surface.width(), surface.height())
}

fn fill_rect(surface: &mut Surface<'_>, rect: Rect, pixel: u32) {
    if rect.x() < 0 || rect.y() < 0 { return; }
    surface.fill_rect(rect.x() as u32, rect.y() as u32, rect.width(), rect.height(), pixel);
}

pub(crate) fn blend(destination: u32, source: u32) -> u32 {
    let alpha = source >> 24;
    if alpha == 0 { return destination; }
    if alpha == 255 { return 0xFF00_0000 | (source & 0x00FF_FFFF); }

    let inverse = 255 - alpha;
    let channel = |shift: u32| -> u32 {
        let src = (source >> shift) & 0xFF;
        let dst = (destination >> shift) & 0xFF;
        (src * alpha + dst * inverse + 127) / 255
    };

    0xFF00_0000 | (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{geometry::Rect, surface::Surface, window::{Window, WindowFlags}};

    fn unshadowed(id: u32, frame: Rect) -> Window {
        let mut window = Window::new(id, frame);
        window.set_flags(WindowFlags { shadow: ShadowStyle::None, ..WindowFlags::default() });
        window
    }

    #[test]
    fn front_layer_overwrites_back_layer() {
        // About z-order, not shadows: these windows cast none.
        let mut framebuffer_pixels = [0u32; 1600];
        let mut framebuffer = Surface::new(&mut framebuffer_pixels, 40, 40).unwrap();
        let mut back_pixels = [0xFFFF_0000u32; 144];
        let back_surface = Surface::new(&mut back_pixels, 12, 12).unwrap();
        let mut front_pixels = [0xFF00_FF00u32; 144];
        let front_surface = Surface::new(&mut front_pixels, 12, 12).unwrap();
        let back = unshadowed(1, Rect::new(5, 5, 12, 12));
        let front = unshadowed(2, Rect::new(10, 10, 12, 12));
        let layers = [Layer::new(&back, &back_surface), Layer::new(&front, &front_surface)];

        Compositor::new(0xFF00_0000).compose(&mut framebuffer, &layers, Rect::new(0, 0, 40, 40));

        assert_eq!(framebuffer.pixel(6, 6), Some(0xFFFF_0000), "back-only area shows back's content");
        assert_eq!(framebuffer.pixel(12, 12), Some(0xFF00_FF00), "the overlap shows front's content");
        assert_eq!(framebuffer.pixel(0, 0), Some(0xFF00_0000), "far from either window: plain background");
    }

    /// One window's shadow on a mid-gray background, 80x80, frame at
    /// (30, 20) 20x20. Scale 1000: extent 22, offset 8.
    fn shadowed(key: bool, radius: u32) -> [u32; 6400] {
        const BACKGROUND: u32 = 0xFFC0_C0C0;
        let mut framebuffer_pixels = [0u32; 6400];
        {
            let mut framebuffer = Surface::new(&mut framebuffer_pixels, 80, 80).unwrap();
            let mut pixels = [0xFFFF_FFFFu32; 400];
            let surface = Surface::new(&mut pixels, 20, 20).unwrap();
            let mut window = Window::new(1, Rect::new(30, 20, 20, 20));
            window.set_flags(WindowFlags { key, corner_radius: radius, ..WindowFlags::default() });
            let layers = [Layer::new(&window, &surface)];
            Compositor::new(BACKGROUND).compose(&mut framebuffer, &layers, Rect::new(0, 0, 80, 80));
        }
        framebuffer_pixels
    }

    fn at(pixels: &[u32; 6400], x: usize, y: usize) -> u32 {
        pixels[y * 80 + x] & 0xFF
    }

    #[test]
    fn the_shadow_falls_below_the_window_and_fades_out() {
        let pixels = shadowed(true, 0);
        assert_eq!(at(&pixels, 40, 30), 0xFF, "never over the window's own pixels");
        let below = at(&pixels, 40, 41);
        let above = at(&pixels, 40, 19);
        assert!(below < above, "light from above: darker under the window ({below}) than over it ({above})");
        assert!(at(&pixels, 40, 50) > below, "it fades with distance");
        assert_eq!(at(&pixels, 40, 79), 0xC0, "and is gone past its reach");
        assert_eq!(at(&pixels, 5, 30), 0xC0, "left of its reach: untouched");
    }

    #[test]
    fn the_key_window_casts_the_darker_shadow() {
        let key = shadowed(true, 0);
        let other = shadowed(false, 0);
        assert!(at(&key, 40, 43) < at(&other, 40, 43));
    }

    #[test]
    fn the_shadow_curves_around_rounded_corners_and_shows_through_them() {
        let square = shadowed(true, 0);
        let rounded = shadowed(true, 8);
        // Just past the bottom-right corner: farther from a rounded boundary.
        assert!(at(&rounded, 51, 44) > at(&square, 51, 44));
        // Straight edges are the same either way.
        assert_eq!(at(&rounded, 40, 44), at(&square, 40, 44));
    }

    #[test]
    fn the_cached_mask_draws_exactly_what_the_direct_path_draws() {
        for (key, radius) in [(true, 0), (false, 8), (true, 10)] {
            let direct = shadowed(key, radius);
            let mut cached = [0u32; 6400];
            {
                let mut framebuffer = Surface::new(&mut cached, 80, 80).unwrap();
                let mut pixels = [0xFFFF_FFFFu32; 400];
                let surface = Surface::new(&mut pixels, 20, 20).unwrap();
                let mut window = Window::new(1, Rect::new(30, 20, 20, 20));
                window.set_flags(WindowFlags { key, corner_radius: radius, ..WindowFlags::default() });
                let mask = ShadowMask::refresh(&window, None);
                let layers = [Layer::new(&window, &surface).with_shadow(mask.as_ref())];
                Compositor::new(0xFFC0_C0C0).compose(&mut framebuffer, &layers, Rect::new(0, 0, 80, 80));
            }
            assert!(direct.iter().zip(cached.iter()).all(|(a, b)| a == b), "key {key} radius {radius}");
        }
    }

    #[test]
    fn isqrt_is_the_floor_square_root() {
        for value in (0u64..70_000).chain([u32::MAX as u64, u64::MAX, (1u64 << 62) - 1, 1u64 << 62]) {
            let root = isqrt(value);
            assert!(root.checked_mul(root).is_some_and(|square| square <= value), "{value}");
            assert!((root + 1).checked_mul(root + 1).is_none_or(|square| square > value), "{value}");
        }
    }

    #[test]
    fn rounded_distance_is_exact_along_edges_and_radial_at_corners() {
        // A 10x10 square at the origin, radius 0: 3 px right of its right edge.
        assert_eq!(rounded_distance(12, 5, 0, 0, 10, 10, 0), 3);
        assert_eq!(rounded_distance(5, 5, 0, 0, 10, 10, 0), 0);
        // Radius 4, diagonally out from the corner arc's centre (5, 5)+...: 3-4-5.
        assert_eq!(rounded_distance(9 + 3, 9 + 4, 0, 0, 14, 14, 4), 1);
    }

    #[test]
    fn damage_limits_composition() {
        let mut framebuffer_pixels = [0u32; 64];
        let mut framebuffer = Surface::new(&mut framebuffer_pixels, 8, 8).unwrap();
        let mut pixels = [0xFFFF_FFFFu32; 16];
        let surface = Surface::new(&mut pixels, 4, 4).unwrap();
        let window = Window::new(1, Rect::new(2, 2, 4, 4));
        let layers = [Layer::new(&window, &surface)];

        Compositor::new(0xFF11_2233).compose(&mut framebuffer, &layers, Rect::new(3, 3, 1, 1));

        assert_eq!(framebuffer.pixel(3, 3), Some(0xFFFF_FFFF));
        assert_eq!(framebuffer.pixel(2, 2), Some(0));
    }
}
