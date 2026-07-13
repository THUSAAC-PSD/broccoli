//! Output-capture helpers shared by the sandbox backends.
//!
//! Both the isolate and mock backends persist a capped, sanitized text
//! preview of a step's stdout/stderr. Keeping the helpers here (rather than
//! per-backend copies) guarantees the NUL stripping below applies uniformly
//! no matter which backend captured the output.

use std::path::Path;
use tokio::io::AsyncReadExt;

pub(super) const INLINE_OUTPUT_PREVIEW_BYTES: usize = 64 * 1024;

pub(super) fn text_preview_from_bytes(mut bytes: Vec<u8>, truncated: bool) -> String {
    let was_truncated = truncated || bytes.len() > INLINE_OUTPUT_PREVIEW_BYTES;
    if bytes.len() > INLINE_OUTPUT_PREVIEW_BYTES {
        bytes.truncate(INLINE_OUTPUT_PREVIEW_BYTES);
    }

    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    // `from_utf8_lossy` preserves NUL (0x00) bytes — they are valid UTF-8
    // (U+0000) — but PostgreSQL TEXT columns reject 0x00. Sandbox stdout/stderr
    // and isolate `meta` can carry stray NUL (truncated multibyte sequences,
    // partially-flushed buffers, control bytes), so strip them at the capture
    // origin rather than relying on every downstream persistence layer. Replace
    // with U+FFFD to match the SDK/host sanitizers.
    if text.contains('\0') {
        text = text.replace('\0', "\u{FFFD}");
    }
    if was_truncated {
        text.push_str("\n... (truncated)");
    }
    text
}

pub(super) async fn read_text_preview(path: &Path) -> Result<String, std::io::Error> {
    let file = tokio::fs::File::open(path).await?;
    let mut bytes = Vec::with_capacity(INLINE_OUTPUT_PREVIEW_BYTES + 1);
    let mut limited = file.take((INLINE_OUTPUT_PREVIEW_BYTES + 1) as u64);
    limited.read_to_end(&mut bytes).await?;
    let truncated = bytes.len() > INLINE_OUTPUT_PREVIEW_BYTES;
    Ok(text_preview_from_bytes(bytes, truncated))
}

#[cfg(test)]
mod text_preview_tests {
    use super::text_preview_from_bytes;

    #[test]
    fn strips_nul_bytes_from_captured_text() {
        // NUL in sandbox stdout/stderr/meta must not survive to a Postgres TEXT column.
        let out = text_preview_from_bytes(b"abc\0def\0\0xyz".to_vec(), false);
        assert_eq!(out, "abc\u{FFFD}def\u{FFFD}\u{FFFD}xyz");
        assert!(!out.contains('\0'));
    }

    #[test]
    fn clean_text_passes_through_unchanged() {
        let out = text_preview_from_bytes(b"999 998 997\n".to_vec(), false);
        assert_eq!(out, "999 998 997\n");
    }

    #[test]
    fn nul_strip_applies_before_truncation_marker() {
        let out = text_preview_from_bytes(b"a\0b".to_vec(), true);
        assert_eq!(out, "a\u{FFFD}b\n... (truncated)");
    }
}
