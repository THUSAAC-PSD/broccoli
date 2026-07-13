//! Soft-wrap highlighted lines into fixed-width rows, keeping per-token colors.

use super::highlight::Span;
use super::pdf::char_cols;

/// `number` is set on the first row of a source line; continuations carry None.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualLine {
    pub number: Option<usize>,
    pub spans: Vec<Span>,
}

/// Coalesces pushed characters into color runs.
#[derive(Default)]
struct RowBuilder {
    spans: Vec<Span>,
}

impl RowBuilder {
    fn push(&mut self, ch: char, color: (u8, u8, u8)) {
        match self.spans.last_mut() {
            Some(last) if last.color == color => last.text.push(ch),
            _ => self.spans.push(Span {
                text: ch.to_string(),
                color,
            }),
        }
    }

    fn take(&mut self) -> Vec<Span> {
        std::mem::take(&mut self.spans)
    }
}

pub fn wrap(lines: &[Vec<Span>], max_cols: usize) -> Vec<VisualLine> {
    let max_cols = max_cols.max(1);
    let mut out = Vec::new();

    if lines.is_empty() {
        out.push(VisualLine {
            number: Some(1),
            spans: Vec::new(),
        });
        return out;
    }

    for (i, line) in lines.iter().enumerate() {
        let mut builder = RowBuilder::default();
        let mut col = 0;
        let mut first = true;

        for span in line {
            for ch in span.text.chars() {
                let w = char_cols(ch);
                // Wrap before a glyph that would overflow so wide chars stay whole.
                if col > 0 && col + w > max_cols {
                    out.push(VisualLine {
                        number: if first { Some(i + 1) } else { None },
                        spans: builder.take(),
                    });
                    first = false;
                    col = 0;
                }
                builder.push(ch, span.color);
                col += w;
            }
        }

        // Emit the trailing or empty row so blank source lines survive.
        if first || !builder.spans.is_empty() {
            out.push(VisualLine {
                number: if first { Some(i + 1) } else { None },
                spans: builder.take(),
            });
        }
    }

    out
}

pub fn paginate(lines: Vec<VisualLine>, lines_per_page: usize) -> Vec<Vec<VisualLine>> {
    let per = lines_per_page.max(1);
    if lines.is_empty() {
        return vec![Vec::new()];
    }
    lines.chunks(per).map(|c| c.to_vec()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> Vec<Span> {
        vec![Span {
            text: text.to_string(),
            color: (0, 0, 0),
        }]
    }

    #[test]
    fn short_lines_keep_their_numbers() {
        let lines = vec![plain("alpha"), plain("beta")];
        let rows = wrap(&lines, 80);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].number, Some(1));
        assert_eq!(rows[1].number, Some(2));
    }

    #[test]
    fn long_line_wraps_with_continuation_rows() {
        let lines = vec![plain(&"x".repeat(200))];
        let rows = wrap(&lines, 90);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].number, Some(1));
        assert_eq!(rows[1].number, None);
        assert_eq!(rows[2].number, None);
    }

    #[test]
    fn cjk_wraps_by_display_width() {
        // 50 full-width chars span 100 cols, so 90 cols holds 45 of them.
        let lines = vec![plain(&"好".repeat(50))];
        let rows = wrap(&lines, 90);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].number, Some(1));
        assert_eq!(rows[1].number, None);
        let row0_chars: usize = rows[0].spans.iter().map(|s| s.text.chars().count()).sum();
        assert_eq!(row0_chars, 45);
    }

    #[test]
    fn blank_line_is_preserved() {
        let lines = vec![plain("a"), vec![], plain("c")];
        let rows = wrap(&lines, 80);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].number, Some(2));
        assert!(rows[1].spans.is_empty());
    }

    #[test]
    fn empty_input_yields_one_row() {
        let rows = wrap(&[], 80);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn mixed_cjk_and_ascii_counts_display_width() {
        let lines = vec![plain("a好b")];
        let rows = wrap(&lines, 80);
        assert_eq!(rows.len(), 1);
        let total_chars: usize = rows[0].spans.iter().map(|s| s.text.chars().count()).sum();
        assert_eq!(total_chars, 3);
    }

    #[test]
    fn cjk_never_splits_across_wrap() {
        // At 3 cols, "a好" fills the row and the second 好 wraps.
        let lines = vec![plain("a好好")];
        let rows = wrap(&lines, 3);
        assert_eq!(rows.len(), 2);
        let row0: String = rows[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(row0.chars().count(), 2, "first row should have 2 chars");
        let row1: String = rows[1].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(row1, "好");
    }

    #[test]
    fn very_long_token_wraps_correctly() {
        let token = "x".repeat(200);
        let lines = vec![plain(&token)];
        let rows = wrap(&lines, 90);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].number, Some(1));
        assert_eq!(rows[1].number, None);
        assert_eq!(rows[2].number, None);
    }

    #[test]
    fn combining_marks_are_zero_width() {
        // Combining acute over e counts as one display column.
        let text = "e\u{0301}";
        let lines = vec![plain(text)];
        let rows = wrap(&lines, 80);
        let row_chars: String = rows[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(row_chars.chars().count(), 2);
    }

    #[test]
    fn empty_line_between_code_lines() {
        let lines = vec![plain("a"), vec![], plain("c")];
        let rows = wrap(&lines, 80);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].number, Some(2));
        assert!(rows[1].spans.is_empty());
    }

    #[test]
    fn pagination_chunks_rows() {
        let rows: Vec<VisualLine> = (0..130)
            .map(|i| VisualLine {
                number: Some(i + 1),
                spans: Vec::new(),
            })
            .collect();
        let pages = paginate(rows, 54);
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].len(), 54);
        assert_eq!(pages[2].len(), 130 - 108);
    }

    #[test]
    fn color_runs_coalesce() {
        let line = vec![
            Span {
                text: "ab".into(),
                color: (1, 2, 3),
            },
            Span {
                text: "cd".into(),
                color: (1, 2, 3),
            },
            Span {
                text: "ef".into(),
                color: (9, 9, 9),
            },
        ];
        let rows = wrap(&[line], 80);
        assert_eq!(rows[0].spans.len(), 2); // abcd + ef
        assert_eq!(rows[0].spans[0].text, "abcd");
    }
}
