//! PDF drawing via printpdf with an embedded Sarasa Fixed SC font. Latin glyphs
//! are half-width and CJK full-width, so display-width layout prints CJK, Greek,
//! Cyrillic, and accents cleanly rather than Windows-1252 only.

use std::io::{BufWriter, Cursor, Write};

use anyhow::Result;
use printpdf::{
    BuiltinFont, Color, IndirectFontRef, Line, Mm, PdfDocument, PdfLayerReference, Point, Rgb,
};
use unicode_width::UnicodeWidthChar;

use super::layout::VisualLine;

const FONT_BYTES: &[u8] = include_bytes!("../../assets/SarasaFixedSC-Regular.ttf");

const PT_TO_MM: f64 = 0.352_777_8;
/// One layout column equals the font's half-width advance.
const CELL_EM: f64 = 0.5;

/// Layout columns for a char, where CJK is 2 and combining marks are 0.
pub fn char_cols(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

fn display_cols(s: &str) -> usize {
    s.chars().map(char_cols).sum()
}

/// Shown in the header and footer bands.
pub struct DocMeta {
    pub banner: String,
    pub problem_label: Option<String>,
    pub who: String,
    pub filename: String,
    pub when: String,
    pub job_id: i64,
}

pub struct PageGeom {
    pub w: f64,
    pub h: f64,
    pub margin_left: f64,
    pub text_left: f64,
    pub margin_right: f64,
    pub first_baseline: f64,
    pub line_h: f64,
    pub char_w: f64,
    pub max_cols: usize,
    pub lines_per_page: usize,
}

fn paper_dims(paper: &str) -> (f64, f64) {
    match paper.to_ascii_uppercase().as_str() {
        "LETTER" => (215.9, 279.4),
        "LEGAL" => (215.9, 355.6),
        _ => (210.0, 297.0), // A4
    }
}

pub fn geometry(paper: &str, font_size: f64) -> PageGeom {
    let (w, h) = paper_dims(paper);
    let char_w = CELL_EM * font_size * PT_TO_MM;
    let line_h = 1.32 * font_size * PT_TO_MM;
    let margin_left = 14.0;
    let text_left = margin_left + 6.0 * char_w + 2.0; // leaves room for the gutter
    let margin_right = 12.0;
    let margin_top = 18.0;
    let margin_bottom = 14.0;
    let first_baseline = h - margin_top;
    let min_baseline = margin_bottom + 2.0;
    let usable = w - text_left - margin_right;
    let max_cols = ((usable / char_w).floor() as i64).max(20) as usize;
    let lines_per_page =
        (((first_baseline - min_baseline) / line_h).floor() as i64).max(1) as usize;
    PageGeom {
        w,
        h,
        margin_left,
        text_left,
        margin_right,
        first_baseline,
        line_h,
        char_w,
        max_cols,
        lines_per_page,
    }
}

fn mm(v: f64) -> Mm {
    Mm(v as f32)
}

fn rgb(c: (u8, u8, u8)) -> Color {
    Color::Rgb(Rgb::new(
        c.0 as f32 / 255.0,
        c.1 as f32 / 255.0,
        c.2 as f32 / 255.0,
        None,
    ))
}

fn gray(v: f32) -> Color {
    Color::Rgb(Rgb::new(v, v, v, None))
}

fn text_width(s: &str, char_w: f64) -> f64 {
    display_cols(s) as f64 * char_w
}

fn draw_rule(layer: &PdfLayerReference, x1: f64, x2: f64, y: f64) {
    let line = Line {
        points: vec![
            (Point::new(mm(x1), mm(y)), false),
            (Point::new(mm(x2), mm(y)), false),
        ],
        is_closed: false,
    };
    layer.set_outline_thickness(0.4);
    layer.set_outline_color(gray(0.8));
    layer.add_line(line);
}

fn draw_header(
    layer: &PdfLayerReference,
    font: &IndirectFontRef,
    geom: &PageGeom,
    meta: &DocMeta,
    font_size: f64,
) {
    let hs = font_size.max(9.0);
    let hsf = hs as f32;
    let hw = CELL_EM * hs * PT_TO_MM;
    let y = geom.h - 10.0;

    if !meta.banner.is_empty() {
        layer.set_fill_color(gray(0.45));
        layer.use_text(meta.banner.as_str(), hsf, mm(geom.margin_left), mm(y), font);
    }

    let center = match &meta.problem_label {
        Some(l) if !l.is_empty() => format!("{l}  \u{b7}  {}", meta.filename),
        _ => meta.filename.clone(),
    };
    let cx = ((geom.w - display_cols(&center) as f64 * hw) / 2.0).max(geom.margin_left);
    layer.set_fill_color(gray(0.12));
    layer.use_text(center.as_str(), hsf, mm(cx), mm(y), font);

    if !meta.who.is_empty() {
        let rx = geom.w - geom.margin_right - display_cols(&meta.who) as f64 * hw;
        layer.set_fill_color(gray(0.45));
        layer.use_text(
            meta.who.as_str(),
            hsf,
            mm(rx.max(geom.margin_left)),
            mm(y),
            font,
        );
    }

    draw_rule(
        layer,
        geom.margin_left,
        geom.w - geom.margin_right,
        geom.h - 13.0,
    );
}

fn draw_footer(
    layer: &PdfLayerReference,
    font: &IndirectFontRef,
    geom: &PageGeom,
    meta: &DocMeta,
    page: usize,
    total: usize,
    font_size: f64,
) {
    let fs = (font_size - 1.0).max(7.0);
    let fsf = fs as f32;
    let fw = CELL_EM * fs * PT_TO_MM;
    let y = 8.0;
    layer.set_fill_color(gray(0.55));

    layer.use_text(
        "broccoli \u{b7} print",
        fsf,
        mm(geom.margin_left),
        mm(y),
        font,
    );

    let center = format!("#{}  \u{b7}  {}", meta.job_id, meta.when);
    let cx = ((geom.w - display_cols(&center) as f64 * fw) / 2.0).max(geom.margin_left);
    layer.use_text(center.as_str(), fsf, mm(cx), mm(y), font);

    let right = format!("page {page} of {total}");
    let rx = geom.w - geom.margin_right - display_cols(&right) as f64 * fw;
    layer.use_text(
        right.as_str(),
        fsf,
        mm(rx.max(geom.margin_left)),
        mm(y),
        font,
    );

    draw_rule(layer, geom.margin_left, geom.w - geom.margin_right, 12.0);
}

fn draw_row(
    layer: &PdfLayerReference,
    font: &IndirectFontRef,
    geom: &PageGeom,
    row: &VisualLine,
    y: f64,
    font_size: f64,
) {
    let fsf = font_size as f32;
    if let Some(n) = row.number {
        let s = n.to_string();
        let x = (geom.text_left - 2.5 - text_width(&s, geom.char_w)).max(2.0);
        layer.set_fill_color(gray(0.62));
        layer.use_text(s.as_str(), fsf, mm(x), mm(y), font);
    }
    let mut x = geom.text_left;
    for span in &row.spans {
        if !span.text.trim().is_empty() {
            layer.set_fill_color(rgb(span.color));
            layer.use_text(span.text.as_str(), fsf, mm(x), mm(y), font);
        }
        x += text_width(&span.text, geom.char_w);
    }
}

pub fn render_pages(
    pages: &[Vec<VisualLine>],
    geom: &PageGeom,
    meta: &DocMeta,
    font_size: f64,
) -> Result<Vec<u8>> {
    let title = format!("print-{}", meta.job_id);
    let (doc, page1, layer1) = PdfDocument::new(&title, mm(geom.w), mm(geom.h), "code");
    // Fall back to Courier so a bad font asset still yields Latin output.
    let font = match doc.add_external_font(Cursor::new(FONT_BYTES)) {
        Ok(f) => f,
        Err(_) => doc.add_builtin_font(BuiltinFont::Courier)?,
    };
    let total = pages.len().max(1);

    for (pi, rows) in pages.iter().enumerate() {
        let layer = if pi == 0 {
            doc.get_page(page1).get_layer(layer1)
        } else {
            let (p, l) = doc.add_page(mm(geom.w), mm(geom.h), "code");
            doc.get_page(p).get_layer(l)
        };

        draw_header(&layer, &font, geom, meta, font_size);
        draw_footer(&layer, &font, geom, meta, pi + 1, total, font_size);

        let mut y = geom.first_baseline;
        for row in rows {
            draw_row(&layer, &font, geom, row, y, font_size);
            y -= geom.line_h;
        }
    }

    let mut bytes = Vec::new();
    {
        let mut writer = BufWriter::new(&mut bytes);
        doc.save(&mut writer)?;
        writer.flush()?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_is_sane_for_a4() {
        let g = geometry("A4", 9.0);
        assert!(g.max_cols >= 90 && g.max_cols <= 140);
        assert!(g.lines_per_page >= 40 && g.lines_per_page <= 80);
    }

    #[test]
    fn cjk_is_double_width() {
        assert_eq!(char_cols('a'), 1);
        assert_eq!(char_cols('好'), 2);
        assert_eq!(display_cols("a好b"), 4);
    }

    #[test]
    fn letter_is_shorter_than_a4() {
        assert!(geometry("Letter", 9.0).lines_per_page <= geometry("A4", 9.0).lines_per_page);
    }
}
