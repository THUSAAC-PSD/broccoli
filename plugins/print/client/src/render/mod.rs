//! Render a code listing to a paginated PDF.

pub mod highlight;
pub mod layout;
mod pdf;

pub use pdf::DocMeta;

use anyhow::Result;

pub struct RenderConfig {
    pub font_size: f32,
    pub paper: String,
}

pub struct Rendered {
    pub bytes: Vec<u8>,
    pub pages: usize,
}

pub fn render(
    source: &str,
    language: &str,
    meta: &DocMeta,
    cfg: &RenderConfig,
) -> Result<Rendered> {
    let font_size = cfg.font_size.clamp(6.0, 16.0) as f64;
    let geom = pdf::geometry(&cfg.paper, font_size);

    let highlighted = highlight::highlight(source, language);
    let rows = layout::wrap(&highlighted, geom.max_cols);
    let pages = layout::paginate(rows, geom.lines_per_page);
    let page_count = pages.len().max(1);

    let bytes = pdf::render_pages(&pages, &geom, meta, font_size)?;
    Ok(Rendered {
        bytes,
        pages: page_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> DocMeta {
        DocMeta {
            banner: "Regionals 2026".into(),
            problem_label: Some("A".into()),
            who: "team-alpha".into(),
            filename: "main.cpp".into(),
            when: "12:34".into(),
            job_id: 7,
        }
    }

    #[test]
    fn renders_a_pdf_document() {
        let r = render(
            "int main() { return 0; }\n",
            "cpp",
            &meta(),
            &RenderConfig {
                font_size: 9.0,
                paper: "A4".into(),
            },
        )
        .unwrap();
        assert!(r.bytes.starts_with(b"%PDF"));
        assert_eq!(r.pages, 1);
    }

    #[test]
    fn long_source_spans_multiple_pages() {
        let source = (0..400)
            .map(|i| format!("line number {i} with some content"))
            .collect::<Vec<_>>()
            .join("\n");
        let r = render(
            &source,
            "text",
            &meta(),
            &RenderConfig {
                font_size: 9.0,
                paper: "A4".into(),
            },
        )
        .unwrap();
        assert!(r.bytes.starts_with(b"%PDF"));
        assert!(r.pages >= 2, "expected multiple pages, got {}", r.pages);
    }
}
