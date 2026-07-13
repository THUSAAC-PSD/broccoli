//! Syntax highlighting via syntect, one colored-span list per source line.
//! Uses a light theme so colors stay legible on white paper.

use syntect::easy::HighlightLines;
use syntect::parsing::{SyntaxReference, SyntaxSet};

/// A run of text sharing one color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub color: (u8, u8, u8),
}

const TAB_WIDTH: usize = 4;

/// Map a broccoli language id to an extension syntect understands.
fn language_extension(language: &str) -> &str {
    match language.to_ascii_lowercase().as_str() {
        "cpp" | "c++" | "cc" | "cxx" => "cpp",
        "c" => "c",
        "python" | "python3" | "py" => "py",
        "java" => "java",
        "javascript" | "js" | "node" => "js",
        "typescript" | "ts" => "ts",
        "rust" | "rs" => "rs",
        "go" | "golang" => "go",
        "kotlin" | "kt" => "kt",
        "csharp" | "cs" => "cs",
        "ruby" | "rb" => "rb",
        "php" => "php",
        "shell" | "bash" | "sh" => "sh",
        _ => "",
    }
}

fn resolve_syntax<'a>(ps: &'a SyntaxSet, language: &str) -> &'a SyntaxReference {
    let ext = language_extension(language);
    if !ext.is_empty() {
        if let Some(s) = ps.find_syntax_by_extension(ext) {
            return s;
        }
    }
    ps.find_syntax_by_token(language)
        .unwrap_or_else(|| ps.find_syntax_plain_text())
}

/// Expand tabs to fixed stops so column math stays simple.
fn expand_tabs(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut col = 0;
    for ch in line.chars() {
        match ch {
            '\t' => {
                let n = TAB_WIDTH - (col % TAB_WIDTH);
                out.extend(std::iter::repeat_n(' ', n));
                col += n;
            }
            '\n' | '\r' => {}
            _ => {
                out.push(ch);
                col += 1;
            }
        }
    }
    out
}

pub fn highlight(source: &str, language: &str) -> Vec<Vec<Span>> {
    let ps = SyntaxSet::load_defaults_nonewlines();
    let ts = syntect::highlighting::ThemeSet::load_defaults();
    let theme = ts
        .themes
        .get("InspiredGitHub")
        .or_else(|| ts.themes.get("base16-ocean.light"))
        .or_else(|| ts.themes.get("Solarized (light)"))
        .or_else(|| {
            // Fall back to any theme with a light background.
            ts.themes.values().find(|t| {
                t.settings.background.is_none_or(|c| {
                    let lum = 0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32;
                    lum > 128.0
                })
            })
        })
        .or_else(|| ts.themes.values().next())
        .expect("at least one default theme");
    let syntax = resolve_syntax(&ps, language);
    let mut hl = HighlightLines::new(syntax, theme);

    let mut out = Vec::new();
    for raw in source.split_inclusive('\n') {
        let expanded = expand_tabs(raw);
        let spans = match hl.highlight_line(&expanded, &ps) {
            Ok(ranges) => ranges
                .into_iter()
                .map(|(style, text)| Span {
                    text: text.to_string(),
                    color: (style.foreground.r, style.foreground.g, style.foreground.b),
                })
                .filter(|s| !s.text.is_empty())
                .collect(),
            Err(_) => vec![Span {
                text: expanded,
                color: (0, 0, 0),
            }],
        };
        out.push(spans);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn maps_languages_to_extensions() {
        assert_eq!(language_extension("python3"), "py");
        assert_eq!(language_extension("CPP"), "cpp");
        assert_eq!(language_extension("mystery"), "");
    }

    #[test]
    fn expands_tabs_to_stops() {
        assert_eq!(expand_tabs("\tx"), "    x");
        assert_eq!(expand_tabs("ab\tc"), "ab  c");
    }

    #[test]
    fn highlights_into_lines() {
        let lines = highlight("int main() {\n  return 0;\n}\n", "cpp");
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().flatten().all(|s| !s.text.is_empty()));
    }

    #[test]
    fn unknown_language_falls_back_to_plain() {
        let lines = highlight("hello world\n", "qwerty");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].iter().any(|s| s.text.contains("hello")));
    }

    #[test]
    fn produces_multiple_colors() {
        let source = "#include <iostream>\nint main() { return 0; }\n";
        let lines = highlight(source, "cpp");
        let colors: HashSet<_> = lines
            .iter()
            .flatten()
            .map(|s| s.color)
            .filter(|c| *c != (0, 0, 0))
            .collect();
        eprintln!("non-black colors for C++: {colors:?}");
        assert!(
            colors.len() > 1,
            "expected multiple colors for syntax-highlighted C++"
        );
    }
}
