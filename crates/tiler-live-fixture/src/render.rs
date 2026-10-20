//! CPU text rendering for the fixture windows.
//!
//! Snapshot lines from [`crate::state::Server::render_lines`] are wrapped to
//! the window width and rasterized with bundled MIT-licensed 8x8 bitmap
//! glyphs ([`font8x8`]). No outline fonts, no font files, no platform text
//! stack. Non-ASCII text renders as `?`; every exposed field stays visible
//! through wrapping plus caller-controlled scroll offset.

/// Glyph cell size in pixels.
pub const GLYPH_W: u32 = 8;
pub const GLYPH_H: u32 = 8;

/// Background and foreground pixel values (`0x00RRGGBB`).
pub const BG: u32 = 0x00141820;
pub const FG: u32 = 0x00e8eef4;
pub const HEADER_FG: u32 = 0x00ffd866;

/// Wrap pre-split lines to `cols` character columns. Long words are split;
/// callers clip vertically with `scroll_rows`.
pub fn wrap_lines(lines: &[String], cols: usize) -> Vec<String> {
    let cols = cols.max(1);
    let mut out = Vec::new();
    for line in lines {
        let ascii = asciify(line);
        if ascii.is_empty() {
            out.push(String::new());
            continue;
        }
        let chars: Vec<char> = ascii.chars().collect();
        for chunk in chars.chunks(cols) {
            out.push(chunk.iter().collect());
        }
    }
    out
}

/// Rasterize wrapped lines into a `width`x`height` pixel buffer starting at
/// `scroll_rows` text rows. Rows beyond the text are left as background.
pub fn rasterize(wrapped: &[String], width: u32, height: u32, scroll_rows: usize) -> Vec<u32> {
    let width = width.max(1) as usize;
    let height = height.max(1) as usize;
    let mut pixels = vec![BG; width * height];
    let cols = (width as u32 / GLYPH_W) as usize;
    let rows = (height as u32 / GLYPH_H) as usize;
    for (row, line) in wrapped.iter().skip(scroll_rows).take(rows).enumerate() {
        let highlight = row == 0 && scroll_rows == 0;
        for (col, ch) in line.chars().take(cols).enumerate() {
            blit_glyph(&mut pixels, width, height, col, row, ch, highlight);
        }
    }
    pixels
}

fn blit_glyph(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    col: usize,
    row: usize,
    ch: char,
    highlight: bool,
) {
    let byte = if (ch as u32) < 128 { ch as u8 } else { b'?' };
    let glyph = font8x8::legacy::BASIC_LEGACY[byte as usize];
    let color = if highlight { HEADER_FG } else { FG };
    for (dy, bits) in glyph.iter().enumerate() {
        for dx in 0..8 {
            if bits & (1 << dx) == 0 {
                continue;
            }
            let x = col * 8 + dx;
            let y = row * 8 + dy;
            if x < width && y < height {
                pixels[y * width + x] = color;
            }
        }
    }
}

fn asciify(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii() { c } else { '?' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_keeps_every_field_visible() {
        let lines = vec![
            "requested size=640x480 mode=windowed".to_string(),
            "description:".to_string(),
        ];
        let wrapped = wrap_lines(&lines, 10);
        assert!(wrapped.len() > lines.len());
        assert_eq!(wrapped.concat().len(), lines.concat().len());
    }

    #[test]
    fn raster_marks_header_and_text_pixels() {
        let wrapped = wrap_lines(
            &[
                "OmniTilerTestWindow:a:run1".to_string(),
                "size=640x480".to_string(),
            ],
            80,
        );
        let pixels = rasterize(&wrapped, 640, 480, 0);
        assert_eq!(pixels.len(), 640 * 480);
        assert!(pixels.contains(&HEADER_FG));
        assert!(pixels.contains(&FG));
        assert!(pixels.contains(&BG));
    }

    #[test]
    fn scroll_offset_moves_content() {
        let lines: Vec<String> = (0..50).map(|i| format!("row-{i:02}")).collect();
        let wrapped = wrap_lines(&lines, 80);
        let top = rasterize(&wrapped, 640, 480, 0);
        let scrolled = rasterize(&wrapped, 640, 480, 20);
        assert_ne!(top, scrolled);
    }

    #[test]
    fn non_ascii_becomes_question_marks() {
        let wrapped = wrap_lines(&["h\u{e9}llo w\u{f6}rld \u{2713}".to_string()], 80);
        assert!(wrapped[0].is_ascii());
    }
}
