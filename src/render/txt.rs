use crate::render::texture::{MipCount, Texture};
use crate::wrappers::graphics::batching::UploadBatch;
use glam::{Vec2, Vec4};

pub fn build_ascii_texture(batch: &mut UploadBatch) -> anyhow::Result<Texture> {
    // ALL_CHARS.len() * 32 by 8 texture
    let mut out_pixels: [u8; font8x8::BASIC_UNICODE.len() * 8 * 4 * 8] = [0; font8x8::BASIC_UNICODE.len() * 8 * 4 * 8];
    for (char_idx, char) in font8x8::BASIC_UNICODE.iter().enumerate() {
        for (line_idx, line) in char.1.iter().enumerate() {
            for bit in 0..8 {
                if *line & 1 << bit != 0 {
                    let x = char_idx * 8 + bit;
                    let i = (line_idx * font8x8::BASIC_UNICODE.len() * 8 + x) * 4;
                    out_pixels[i + 0] = 255;
                    out_pixels[i + 1] = 255;
                    out_pixels[i + 2] = 255;
                    out_pixels[i + 3] = 255;
                }
            }
        }
    }

    Texture::from_rgba8(batch, (font8x8::BASIC_UNICODE.len() * 8) as u32, 8, &out_pixels, false, MipCount::Limit(1), "Font")
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Glyph {
    pub pos: Vec2,
    pub scale: f32,
    pub code: u32,
    pub color: Vec4,
}

pub fn display_text(text: &str, width: u32, scale: f32, color: Vec4, drop_shadow: bool) -> Vec<Glyph> {
    let pos = calc_right_position(text, width, scale);
    let mut glyphs = Vec::with_capacity(text.len());
    if drop_shadow {
        calc_text_layout(&mut glyphs, text, pos + Vec2::new(2., 2.), scale, Vec4::new(0., 0., 0., 1.));
    }
    calc_text_layout(&mut glyphs, text, pos, scale, color);
    glyphs
}

fn calc_right_position(text: &str, width: u32, scale: f32) -> Vec2 {
    let longest_line = text.split("\n").into_iter().map(|it| it.len()).max().unwrap();
    let width_in_pixels = longest_line as f32 * 8. * scale;
    Vec2::new(width as f32 - width_in_pixels, 8.)
}

fn calc_text_layout(out: &mut Vec<Glyph>, text: &str, pos: Vec2, scale: f32, color: Vec4) {
    let mut p = pos;
    for c in text.chars() {
        if c == '\n' { p = Vec2::new(pos.x, p.y + 8. * scale); continue; }
        if c != ' ' {
            let code = if c.is_ascii() { c as u32 } else { '?' as u32 };
            out.push(Glyph { pos: p, scale, code: code as u32, color });
        }
        p.x += 8. * scale;
    }
}