use ratatui::style::Color;
use ratatui::symbols::border::Set as BorderSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    NoColor,
    Ansi16,
    Ansi256,
    Truecolor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphSet {
    Unicode,
    Ascii,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorToken {
    Accent,
    Ok,
    Warn,
    Err,
    Dim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseGlyph {
    Pending,
    Running,
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy)]
pub struct BorderChars {
    pub top_left: char,
    pub top_right: char,
    pub bottom_left: char,
    pub bottom_right: char,
    pub horizontal: char,
    pub vertical: char,
}

#[derive(Debug, Clone)]
pub struct Theme {
    capability: Capability,
    glyphs: GlyphSet,
}

impl Theme {
    pub fn new(capability: Capability, glyphs: GlyphSet) -> Self {
        Self { capability, glyphs }
    }

    pub fn from_env() -> Self {
        let no_color = std::env::var("NO_COLOR").ok();
        let colorterm = std::env::var("COLORTERM").ok();
        let term = std::env::var("TERM").ok();
        let lang = std::env::var("LANG").ok();
        let lc_ctype = std::env::var("LC_CTYPE").ok();
        Self {
            capability: detect_capability(
                no_color.as_deref(),
                colorterm.as_deref(),
                term.as_deref(),
            ),
            glyphs: detect_glyphs(lang.as_deref(), lc_ctype.as_deref()),
        }
    }

    pub fn capability(&self) -> Capability {
        self.capability
    }

    pub fn glyphs(&self) -> GlyphSet {
        self.glyphs
    }

    pub fn color(&self, token: ColorToken) -> Color {
        match self.capability {
            Capability::NoColor => Color::Reset,
            Capability::Ansi16 => match token {
                ColorToken::Accent => Color::Cyan,
                ColorToken::Ok => Color::Green,
                ColorToken::Warn => Color::Yellow,
                ColorToken::Err => Color::Red,
                ColorToken::Dim => Color::DarkGray,
            },
            Capability::Ansi256 => match token {
                ColorToken::Accent => Color::Indexed(81),
                ColorToken::Ok => Color::Indexed(71),
                ColorToken::Warn => Color::Indexed(179),
                ColorToken::Err => Color::Indexed(167),
                ColorToken::Dim => Color::Indexed(242),
            },
            Capability::Truecolor => match token {
                ColorToken::Accent => Color::Rgb(0x5f, 0xd7, 0xff),
                ColorToken::Ok => Color::Rgb(0x5f, 0xaf, 0x5f),
                ColorToken::Warn => Color::Rgb(0xd7, 0xaf, 0x5f),
                ColorToken::Err => Color::Rgb(0xd7, 0x5f, 0x5f),
                ColorToken::Dim => Color::Rgb(0x6c, 0x6c, 0x6c),
            },
        }
    }

    pub fn phase_glyph(&self, g: PhaseGlyph) -> &'static str {
        match (self.glyphs, g) {
            (GlyphSet::Unicode, PhaseGlyph::Pending) => "\u{25CB}",
            (GlyphSet::Unicode, PhaseGlyph::Running) => "\u{25B6}",
            (GlyphSet::Unicode, PhaseGlyph::Passed) => "\u{2713}",
            (GlyphSet::Unicode, PhaseGlyph::Failed) => "\u{2717}",
            (GlyphSet::Unicode, PhaseGlyph::Skipped) => "\u{2500}",
            (GlyphSet::Ascii, PhaseGlyph::Pending) => "[ ]",
            (GlyphSet::Ascii, PhaseGlyph::Running) => "[*]",
            (GlyphSet::Ascii, PhaseGlyph::Passed) => "[x]",
            (GlyphSet::Ascii, PhaseGlyph::Failed) => "[!]",
            (GlyphSet::Ascii, PhaseGlyph::Skipped) => "[-]",
        }
    }

    pub fn outer_border(&self) -> BorderChars {
        match self.glyphs {
            GlyphSet::Unicode => BorderChars {
                top_left: '\u{2554}',
                top_right: '\u{2557}',
                bottom_left: '\u{255A}',
                bottom_right: '\u{255D}',
                horizontal: '\u{2550}',
                vertical: '\u{2551}',
            },
            GlyphSet::Ascii => BorderChars {
                top_left: '+',
                top_right: '+',
                bottom_left: '+',
                bottom_right: '+',
                horizontal: '=',
                vertical: '|',
            },
        }
    }

    pub fn inner_border(&self) -> BorderChars {
        match self.glyphs {
            GlyphSet::Unicode => BorderChars {
                top_left: '\u{250C}',
                top_right: '\u{2510}',
                bottom_left: '\u{2514}',
                bottom_right: '\u{2518}',
                horizontal: '\u{2500}',
                vertical: '\u{2502}',
            },
            GlyphSet::Ascii => BorderChars {
                top_left: '+',
                top_right: '+',
                bottom_left: '+',
                bottom_right: '+',
                horizontal: '-',
                vertical: '|',
            },
        }
    }

    pub fn outer_border_set(&self) -> BorderSet {
        match self.glyphs {
            GlyphSet::Unicode => BorderSet {
                top_left: "\u{2554}",
                top_right: "\u{2557}",
                bottom_left: "\u{255A}",
                bottom_right: "\u{255D}",
                vertical_left: "\u{2551}",
                vertical_right: "\u{2551}",
                horizontal_top: "\u{2550}",
                horizontal_bottom: "\u{2550}",
            },
            GlyphSet::Ascii => BorderSet {
                top_left: "+",
                top_right: "+",
                bottom_left: "+",
                bottom_right: "+",
                vertical_left: "|",
                vertical_right: "|",
                horizontal_top: "=",
                horizontal_bottom: "=",
            },
        }
    }

    pub fn inner_border_set(&self) -> BorderSet {
        match self.glyphs {
            GlyphSet::Unicode => BorderSet {
                top_left: "\u{250C}",
                top_right: "\u{2510}",
                bottom_left: "\u{2514}",
                bottom_right: "\u{2518}",
                vertical_left: "\u{2502}",
                vertical_right: "\u{2502}",
                horizontal_top: "\u{2500}",
                horizontal_bottom: "\u{2500}",
            },
            GlyphSet::Ascii => BorderSet {
                top_left: "+",
                top_right: "+",
                bottom_left: "+",
                bottom_right: "+",
                vertical_left: "|",
                vertical_right: "|",
                horizontal_top: "-",
                horizontal_bottom: "-",
            },
        }
    }

    pub fn sparkline_levels(&self) -> &'static [char] {
        match self.glyphs {
            GlyphSet::Unicode => &[
                '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
                '\u{2588}',
            ],
            GlyphSet::Ascii => &['_', '.', '-', '=', '^', '"'],
        }
    }
}

pub fn detect_capability(
    no_color: Option<&str>,
    colorterm: Option<&str>,
    term: Option<&str>,
) -> Capability {
    if no_color.is_some() {
        return Capability::NoColor;
    }
    if let Some(ct) = colorterm {
        let lower = ct.to_ascii_lowercase();
        if lower == "truecolor" || lower == "24bit" {
            return Capability::Truecolor;
        }
    }
    if let Some(t) = term
        && t.contains("256color")
    {
        return Capability::Ansi256;
    }
    Capability::Ansi16
}

pub fn detect_glyphs(lang: Option<&str>, lc_ctype: Option<&str>) -> GlyphSet {
    let utf8 = |s: &str| {
        let lower = s.to_ascii_lowercase();
        lower.contains("utf-8") || lower.contains("utf8")
    };
    if lang.map(utf8).unwrap_or(false) || lc_ctype.map(utf8).unwrap_or(false) {
        GlyphSet::Unicode
    } else {
        GlyphSet::Ascii
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_color_overrides_everything() {
        assert_eq!(
            detect_capability(Some(""), Some("truecolor"), Some("xterm-256color")),
            Capability::NoColor,
        );
        assert_eq!(
            detect_capability(Some("1"), None, None),
            Capability::NoColor,
        );
    }

    #[test]
    fn truecolor_via_colorterm() {
        assert_eq!(
            detect_capability(None, Some("truecolor"), None),
            Capability::Truecolor,
        );
        assert_eq!(
            detect_capability(None, Some("24bit"), None),
            Capability::Truecolor,
        );
        assert_eq!(
            detect_capability(None, Some("Truecolor"), None),
            Capability::Truecolor,
        );
    }

    #[test]
    fn ansi256_via_term() {
        assert_eq!(
            detect_capability(None, None, Some("xterm-256color")),
            Capability::Ansi256,
        );
        assert_eq!(
            detect_capability(None, None, Some("screen-256color")),
            Capability::Ansi256,
        );
        assert_eq!(
            detect_capability(None, None, Some("tmux-256color")),
            Capability::Ansi256,
        );
    }

    #[test]
    fn ansi16_when_nothing_indicates_more() {
        assert_eq!(
            detect_capability(None, None, Some("xterm")),
            Capability::Ansi16,
        );
        assert_eq!(
            detect_capability(None, None, Some("dumb")),
            Capability::Ansi16
        );
        assert_eq!(detect_capability(None, None, None), Capability::Ansi16);
    }

    #[test]
    fn unicode_glyphs_when_lang_has_utf8() {
        assert_eq!(detect_glyphs(Some("en_US.UTF-8"), None), GlyphSet::Unicode);
        assert_eq!(detect_glyphs(Some("en_US.utf-8"), None), GlyphSet::Unicode);
        assert_eq!(detect_glyphs(Some("zh_CN.UTF8"), None), GlyphSet::Unicode);
    }

    #[test]
    fn unicode_glyphs_via_lc_ctype() {
        assert_eq!(detect_glyphs(None, Some("UTF-8")), GlyphSet::Unicode);
        assert_eq!(
            detect_glyphs(Some("C"), Some("en_US.UTF-8")),
            GlyphSet::Unicode
        );
    }

    #[test]
    fn ascii_glyphs_when_no_utf8_indicator() {
        assert_eq!(detect_glyphs(Some("C"), None), GlyphSet::Ascii);
        assert_eq!(detect_glyphs(Some("POSIX"), None), GlyphSet::Ascii);
        assert_eq!(detect_glyphs(None, None), GlyphSet::Ascii);
    }
}
