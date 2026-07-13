use ratatui::style::Color;

pub struct Theme {
    pub primary: Color,
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub muted: Color,
    pub accent: Color,
    pub bg: Color,
    pub fg: Color,
    /// Fill behind modal overlays.
    pub overlay_bg: Color,
    /// Drop-shadow behind modal overlays.
    pub shadow: Color,
}

pub const THEME: Theme = Theme {
    primary: Color::Cyan,
    success: Color::Green,
    error: Color::Red,
    warning: Color::Yellow,
    muted: Color::DarkGray,
    accent: Color::Magenta,
    bg: Color::Black,
    fg: Color::White,
    overlay_bg: Color::Reset,
    shadow: Color::DarkGray,
};

pub fn supports_unicode() -> bool {
    std::env::var("TERM")
        .map(|t| !t.contains("dumb") && !t.contains("vt100"))
        .unwrap_or(false)
}
