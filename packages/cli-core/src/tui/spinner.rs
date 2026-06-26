pub struct Spinner {
    frames: &'static [&'static str],
    index: usize,
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Spinner {
    pub fn new() -> Self {
        Self {
            frames: if super::theme::supports_unicode() {
                &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
            } else {
                &["|", "/", "-", "\\"]
            },
            index: 0,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> &str {
        let f = self.frames[self.index];
        self.index = (self.index + 1) % self.frames.len();
        f
    }
}
