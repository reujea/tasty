//! A glyph too big for the font atlas draws nothing instead of panicking.
//!
//! epaint is patched for this in vendor/epaint. The inputs are built from fixed
//! bytes, so these tests do not depend on the fonts of the machine.

use std::sync::Arc;

use egui::epaint::text::{FontData, FontDefinitions, FontFamily};
use egui::epaint::{Primitive, TextureAtlas};
use egui::{Color32, Context, FontId, RawInput};

const VALID: &[u8] = include_bytes!("../../tasty-font/assets/D2Coding-ligature-Regular.ttf");

const ATLAS_SIDE: usize = 2048;
const FAMILY: &str = "probe";
const TEXT_SIZE: f32 = 14.0;
const UNITS_PER_EM: u16 = 16;
const LOW_DENSITY: f32 = 1.0;
const HIGH_DENSITY: f32 = 2.0;
const LARGE_TEXT_SIZE: f32 = 72.0;

/// A TrueType file with one rectangular glyph mapped from each of `chars`.
/// `advance` follows the width, as in a font that really has such a glyph.
fn tiny_font(width: i16, height: i16, chars: &str) -> Vec<u8> {
    rect_font(&[(width, height, chars)])
}

/// A TrueType file with one rectangular glyph per entry of `(width, height, chars)`.
fn rect_font(entries: &[(i16, i16, &str)]) -> Vec<u8> {
    fn be16(out: &mut Vec<u8>, values: &[i32]) {
        for v in values {
            out.extend_from_slice(&(*v as i16).to_be_bytes());
        }
    }
    let upem = i32::from(UNITS_PER_EM);
    let glyph_count = entries.len() as i32 + 1;
    let widest = entries.iter().map(|e| i32::from(e.0)).max().unwrap_or(0);
    let tallest = entries.iter().map(|e| i32::from(e.1)).max().unwrap_or(0);

    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    for offset in [0_u32, 0] {
        loca.extend_from_slice(&offset.to_be_bytes());
    }
    let mut hmtx = Vec::new();
    be16(&mut hmtx, &[0, 0]);
    let mut mapping: Vec<(u32, u32)> = Vec::new();
    for (index, (width, height, chars)) in entries.iter().enumerate() {
        let (w, h) = (i32::from(*width), i32::from(*height));
        be16(&mut glyf, &[1, 0, 0, w, h, 3, 0]);
        glyf.extend_from_slice(&[1; 4]);
        be16(&mut glyf, &[0, w, 0, -w, 0, 0, h, 0]);
        if glyf.len() % 2 == 1 {
            glyf.push(0);
        }
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        be16(&mut hmtx, &[w, 0]);
        mapping.extend(chars.chars().map(|c| (u32::from(c), index as u32 + 1)));
    }

    let mut head = Vec::new();
    head.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
    head.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
    head.extend_from_slice(&0_u32.to_be_bytes());
    head.extend_from_slice(&0x5F0F_3CF5_u32.to_be_bytes());
    be16(&mut head, &[0, upem]);
    head.extend_from_slice(&[0; 16]);
    be16(&mut head, &[0, 0, widest, tallest, 0, 8, 2, 1, 0]);

    let mut hhea = 0x0001_0000_u32.to_be_bytes().to_vec();
    be16(
        &mut hhea,
        &[
            upem,
            -upem / 4,
            0,
            widest,
            0,
            0,
            widest,
            1,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            glyph_count,
        ],
    );

    let mut maxp = 0x0001_0000_u32.to_be_bytes().to_vec();
    be16(
        &mut maxp,
        &[glyph_count, 4, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
    );

    mapping.sort_unstable();
    mapping.dedup_by_key(|m| m.0);
    let mut cmap = Vec::new();
    be16(&mut cmap, &[0, 1, 3, 10]);
    cmap.extend_from_slice(&12_u32.to_be_bytes());
    be16(&mut cmap, &[12, 0]);
    for value in [16 + 12 * mapping.len() as u32, 0, mapping.len() as u32] {
        cmap.extend_from_slice(&value.to_be_bytes());
    }
    for (c, glyph) in &mapping {
        for value in [*c, *c, *glyph] {
            cmap.extend_from_slice(&value.to_be_bytes());
        }
    }

    let tables: [(&[u8; 4], &[u8]); 7] = [
        (b"cmap", &cmap),
        (b"glyf", &glyf),
        (b"head", &head),
        (b"hhea", &hhea),
        (b"hmtx", &hmtx),
        (b"loca", &loca),
        (b"maxp", &maxp),
    ];
    let mut font = 0x0001_0000_u32.to_be_bytes().to_vec();
    be16(&mut font, &[tables.len() as i32, 0, 0, 0]);
    let mut offset = 12 + 16 * tables.len();
    for (tag, table) in tables {
        font.extend_from_slice(tag);
        font.extend_from_slice(&0_u32.to_be_bytes());
        font.extend_from_slice(&(offset as u32).to_be_bytes());
        font.extend_from_slice(&(table.len() as u32).to_be_bytes());
        offset += table.len() + (4 - table.len() % 4) % 4;
    }
    for (_, table) in tables {
        font.extend_from_slice(table);
        font.resize(font.len() + (4 - table.len() % 4) % 4, 0);
    }
    font
}

struct Glyph {
    blank: bool,
    advance: f32,
}

/// Runs what a frame runs: layout, paint, tessellation and the atlas upload,
/// three times, because a glyph that fails is built again on the next pass.
fn draw(font: Vec<u8>, text: &str, size: f32, density: f32) -> Vec<Glyph> {
    draw_measuring_upload(font, text, size, density).0
}

/// Also returns the tallest whole atlas image the frames uploaded.
fn draw_measuring_upload(
    font: Vec<u8>,
    text: &str,
    size: f32,
    density: f32,
) -> (Vec<Glyph>, usize) {
    let mut definitions = FontDefinitions::default();
    definitions
        .font_data
        .insert(FAMILY.to_owned(), Arc::new(FontData::from_owned(font)));
    definitions
        .families
        .insert(FontFamily::Name(FAMILY.into()), vec![FAMILY.to_owned()]);

    let ctx = Context::default();
    ctx.set_fonts(definitions);
    ctx.set_pixels_per_point(density);

    let mut glyphs = Vec::new();
    let mut uploaded_height = 0;
    for _ in 0..3 {
        let raw = RawInput {
            max_texture_side: Some(ATLAS_SIDE),
            ..Default::default()
        };
        let output = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let font = FontId::new(size, FontFamily::Name(FAMILY.into()));
                let galley = ui
                    .painter()
                    .layout_no_wrap(text.to_owned(), font, Color32::WHITE);
                glyphs = galley
                    .rows
                    .iter()
                    .flat_map(|row| &row.glyphs)
                    .map(|glyph| Glyph {
                        blank: glyph.uv_rect.is_nothing(),
                        advance: glyph.advance_width,
                    })
                    .collect();
                ui.painter()
                    .galley(egui::Pos2::ZERO, galley, Color32::WHITE);
            });
        });
        for (_, delta) in &output.textures_delta.set {
            if delta.pos.is_none() {
                uploaded_height = uploaded_height.max(delta.image.height());
            }
        }
        for primitive in ctx.tessellate(output.shapes, output.pixels_per_point) {
            let Primitive::Mesh(mesh) = primitive.primitive else {
                continue;
            };
            assert!(
                mesh.vertices
                    .iter()
                    .all(|v| v.pos.is_finite() && v.uv.is_finite()),
                "tessellation produced a coordinate that is not finite"
            );
        }
    }
    (glyphs, uploaded_height)
}

fn assert_blank_within_one_em(glyphs: &[Glyph], size: f32) {
    assert!(!glyphs.is_empty(), "the text produced no glyphs");
    for glyph in glyphs {
        assert!(glyph.blank, "the oversized glyph was drawn");
        // One em, plus the rounding of the scale to whole pixels.
        assert!(
            glyph.advance <= size + 1.0,
            "advance {} would push the rest of the line away",
            glyph.advance
        );
    }
}

#[test]
fn a_glyph_wider_than_the_atlas_is_blank() {
    let font = tiny_font(30000, 100, "A");
    assert_blank_within_one_em(&draw(font, "A", TEXT_SIZE, LOW_DENSITY), TEXT_SIZE);
}

#[test]
fn a_glyph_taller_than_the_atlas_is_blank() {
    let font = tiny_font(100, 30000, "A");
    assert_blank_within_one_em(&draw(font, "A", TEXT_SIZE, LOW_DENSITY), TEXT_SIZE);
}

#[test]
fn tall_glyphs_that_only_overflow_together_are_blank() {
    let text: String = ('A'..='Z').chain('a'..='z').collect();
    let font = tiny_font(100, 1600, &text);
    assert_blank_within_one_em(&draw(font, &text, TEXT_SIZE, LOW_DENSITY), TEXT_SIZE);
}

// epaint builds the replacement glyph with the family, so these two would panic
// even though the text never asks for the oversized glyph.
#[test]
fn an_oversized_primary_replacement_glyph_does_not_panic() {
    let font = tiny_font(30000, 100, "\u{25FB}");
    assert_blank_within_one_em(&draw(font, "x", TEXT_SIZE, LOW_DENSITY), TEXT_SIZE);
}

#[test]
fn an_oversized_fallback_replacement_glyph_does_not_panic() {
    let font = tiny_font(30000, 100, "?");
    assert_blank_within_one_em(&draw(font, "x", TEXT_SIZE, LOW_DENSITY), TEXT_SIZE);
}

#[test]
fn asking_for_a_glyph_builds_the_family_without_panicking() {
    let mut definitions = FontDefinitions::default();
    definitions.font_data.insert(
        FAMILY.to_owned(),
        Arc::new(FontData::from_owned(tiny_font(30000, 100, "\u{25FB}"))),
    );
    definitions
        .families
        .insert(FontFamily::Name(FAMILY.into()), vec![FAMILY.to_owned()]);
    let ctx = Context::default();
    ctx.set_fonts(definitions);
    drop(ctx.run(RawInput::default(), |_| {}));
    let font = FontId::new(TEXT_SIZE, FontFamily::Name(FAMILY.into()));
    // epaint answers false for the replacement glyph itself, so only reaching the
    // answer is the point here.
    assert!(!ctx.fonts(|f| f.has_glyph(&font, '\u{25FB}')));
}

#[test]
fn a_font_with_a_small_em_blanks_only_at_the_density_that_overflows() {
    // upem 16 on real outlines: everything is 62.5 times its intended size.
    let mut font = VALID.to_vec();
    let tables = usize::from(u16::from_be_bytes([font[4], font[5]]));
    let record = (0..tables)
        .map(|i| 12 + 16 * i)
        .find(|&r| &font[r..r + 4] == b"head")
        .unwrap();
    let head = u32::from_be_bytes(font[record + 8..record + 12].try_into().unwrap()) as usize;
    font[head + 18..head + 20].copy_from_slice(&UNITS_PER_EM.to_be_bytes());

    let text = "AaBbCcDdEeFfGg\u{AC00}\u{B098}\u{B2E4}1234567890";
    let low = draw(font.clone(), text, TEXT_SIZE, LOW_DENSITY);
    assert!(low.iter().all(|g| !g.blank), "fits at this density");
    let high = draw(font, text, TEXT_SIZE, HIGH_DENSITY);
    assert!(high.iter().any(|g| g.blank), "the Hangul does not fit");
}

#[test]
fn a_normal_font_is_drawn_with_its_own_advance() {
    for density in [LOW_DENSITY, HIGH_DENSITY] {
        let glyphs = draw(VALID.to_vec(), "Aa\u{AC00}1", TEXT_SIZE, density);
        assert_eq!(glyphs.len(), 4);
        assert!(glyphs.iter().all(|g| !g.blank));
        // 0.5 em and 1 em, within the rounding of the scale to whole pixels.
        assert!((glyphs[0].advance - TEXT_SIZE / 2.0).abs() < 0.5);
        assert!((glyphs[2].advance - TEXT_SIZE).abs() < 1.0);
    }
}

#[test]
fn a_normal_font_at_the_largest_input_size_is_drawn() {
    let glyphs = draw(VALID.to_vec(), "Aa\u{AC00}1", LARGE_TEXT_SIZE, 4.0);
    assert_eq!(glyphs.len(), 4);
    assert!(glyphs.iter().all(|g| !g.blank));
}

#[test]
fn the_limits_are_the_atlas_width_and_half_of_it() {
    let atlas = TextureAtlas::new([ATLAS_SIDE, 32]);
    assert!(atlas.can_always_allocate((ATLAS_SIDE, 1)));
    assert!(!atlas.can_always_allocate((ATLAS_SIDE + 1, 1)));
    assert!(atlas.can_always_allocate((1, ATLAS_SIDE / 2)));
    assert!(!atlas.can_always_allocate((1, ATLAS_SIDE / 2 + 1)));
}

/// Touches the two far corners, as drawing a glyph does, then takes the delta.
fn place(atlas: &mut TextureAtlas, (w, h): (usize, usize)) {
    let ((x, y), image) = atlas.allocate((w, h));
    image[(x, y)] = 1.0;
    image[(x + w - 1, y + h - 1)] = 1.0;
}

#[test]
fn the_tallest_allowed_glyph_fits_after_any_stack_of_rows() {
    for row_height in [31, 39, 63, 99, 127, 255, 511, 1023] {
        for rows in 0..=ATLAS_SIDE / (row_height + 1) {
            let mut atlas = TextureAtlas::new([ATLAS_SIDE, 32]);
            for _ in 0..rows {
                place(&mut atlas, (ATLAS_SIDE, row_height));
            }
            drop(atlas.take_delta());
            place(&mut atlas, (100, ATLAS_SIDE / 2));
            drop(atlas.take_delta());
        }
    }
}

/// Glyph heights at an atlas of 2048, all as wide as the atlas so each takes a
/// row. The first two overflow, which puts the cursor at a known place whatever
/// was drawn before. The rest then end rows exactly at 2048, 4096 and 8192.
const ROWS_ENDING_AT_THE_IMAGE_HEIGHT: [i16; 9] =
    [1024, 1024, 341, 1024, 1022, 1024, 1024, 1024, 1020];

#[test]
fn rows_ending_at_the_image_height_do_not_grow_it_past_the_width() {
    let mut atlas = TextureAtlas::new([ATLAS_SIDE, 32]);
    for height in ROWS_ENDING_AT_THE_IMAGE_HEIGHT {
        place(&mut atlas, (ATLAS_SIDE, height as usize));
        assert!(atlas.size()[1] <= ATLAS_SIDE, "grew to {:?}", atlas.size());
        drop(atlas.take_delta());
    }
}

#[test]
fn a_font_of_allowed_sizes_never_uploads_an_atlas_past_its_width() {
    // At this size and density one font unit is one pixel.
    const ONE_UNIT_PER_PIXEL: f32 = 16.0;
    let text = "ABCDEFGHI";
    let chars: Vec<String> = text.chars().map(String::from).collect();
    let entries: Vec<(i16, i16, &str)> = ROWS_ENDING_AT_THE_IMAGE_HEIGHT
        .iter()
        .zip(&chars)
        .map(|(height, c)| (ATLAS_SIDE as i16, *height, c.as_str()))
        .collect();
    let (glyphs, uploaded_height) =
        draw_measuring_upload(rect_font(&entries), text, ONE_UNIT_PER_PIXEL, LOW_DENSITY);
    assert_eq!(glyphs.len(), entries.len());
    assert!(
        glyphs.iter().all(|g| !g.blank),
        "an allowed glyph was left blank"
    );
    assert_eq!(uploaded_height, ATLAS_SIDE);
}

#[test]
fn any_order_of_allowed_sizes_stays_in_bounds() {
    fn next(state: &mut u64) -> usize {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state as usize
    }
    let mut state = 0x2545_F491_4F6C_DD1D;
    // 3000 is not a power of two. A limit of half the width alone panics there.
    for side in [1024, 2048, 3000, 4096] {
        for _ in 0..200 {
            let mut atlas = TextureAtlas::new([side, 32]);
            let tallest = [8, 40, 128, 512, side / 2 + 64, side][next(&mut state) % 6];
            for _ in 0..1 + next(&mut state) % 300 {
                let width = if next(&mut state).is_multiple_of(2) {
                    1 + next(&mut state) % (side + 8)
                } else {
                    1 + next(&mut state) % 64
                };
                let size = (width, 1 + next(&mut state) % tallest);
                if atlas.can_always_allocate(size) {
                    place(&mut atlas, size);
                    assert!(atlas.size()[1] <= side.next_power_of_two());
                }
                if next(&mut state).is_multiple_of(8) {
                    drop(atlas.take_delta());
                }
            }
            drop(atlas.take_delta());
        }
    }
}
