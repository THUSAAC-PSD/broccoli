/// Truncate a string to at most `max_chars` Unicode characters.
pub fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}... (truncated)")
    }
}

/// Split a string into whitespace-delimited tokens.
pub fn tokenize(s: &str) -> Vec<&str> {
    s.split_whitespace().collect()
}

/// Split into lines, trim trailing whitespace per line, drop trailing empty lines.
pub fn split_lines_trimmed(s: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = s.lines().map(|l| l.trim_end()).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

pub fn token_count_msg(expected: usize, actual: usize) -> String {
    format!("Token count mismatch: expected {expected} tokens, got {actual}")
}

pub fn token_mismatch_msg(pos: usize, expected: &str, actual: &str) -> String {
    format!(
        "Token mismatch at position {pos}: expected '{}', got '{}'",
        truncate(expected, 50),
        truncate(actual, 50)
    )
}

pub fn line_count_msg(expected: usize, actual: usize) -> String {
    format!("Line count mismatch: expected {expected} lines, got {actual}")
}

pub fn line_mismatch_msg(line: usize, expected: &str, actual: &str) -> String {
    format!(
        "Line {line} differs:\n  expected: '{}'\n  actual:   '{}'",
        truncate(expected, 100),
        truncate(actual, 100)
    )
}

pub fn diff_preview(expected: &str, actual: &str, max_chars: usize) -> String {
    format!(
        "Expected:\n{}\n\nGot:\n{}",
        truncate(expected, max_chars),
        truncate(actual, max_chars)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let result = truncate("hello world", 5);
        assert!(result.starts_with("hello"));
        assert!(result.contains("truncated"));
    }

    #[test]
    fn tokenize_whitespace() {
        assert_eq!(tokenize("  a  b\n\tc "), vec!["a", "b", "c"]);
    }

    #[test]
    fn split_lines_trims_trailing() {
        let lines = split_lines_trimmed("a  \nb\n\n");
        assert_eq!(lines, vec!["a", "b"]);
    }
}
