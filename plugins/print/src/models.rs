//! Shared types and host-free helpers, unit-testable off wasm.

use serde::{Deserialize, Serialize};

pub mod status {
    pub const PENDING_APPROVAL: &str = "pending_approval";
    pub const PENDING: &str = "pending";
    pub const CLAIMED: &str = "claimed";
    pub const PRINTING: &str = "printing";
    pub const DONE: &str = "done";
    pub const FAILED: &str = "failed";
    pub const CANCELED: &str = "canceled";

    /// Statuses a station may report back.
    pub const STATION_SETTABLE: &[&str] = &[PRINTING, DONE, FAILED];

    pub fn is_valid(s: &str) -> bool {
        matches!(
            s,
            PENDING_APPROVAL | PENDING | CLAIMED | PRINTING | DONE | FAILED | CANCELED
        )
    }
}

// Mirror the client's default rendering so the server max_pages guard predicts
// the real page count. The client still enforces its own hard limit.
pub const WRAP_COLS: usize = 90;
pub const LINES_PER_PAGE: usize = 54;

/// Matches the code-run file cap.
pub const MAX_SOURCE_CHARS: usize = 1_000_000;

pub fn estimate_pages(source: &str) -> i32 {
    estimate_pages_with(source, WRAP_COLS, LINES_PER_PAGE)
}

pub fn estimate_pages_with(source: &str, cols: usize, lines_per_page: usize) -> i32 {
    let cols = cols.max(1);
    let lines_per_page = lines_per_page.max(1);
    // Floor at one line so an empty file still counts as a page.
    let visual_lines: usize = source
        .split('\n')
        .map(|line| {
            let width = line.chars().count();
            if width == 0 { 1 } else { width.div_ceil(cols) }
        })
        .sum::<usize>()
        .max(1);
    visual_lines.div_ceil(lines_per_page).max(1) as i32
}

/// Fallback label (A, B, ... Z, AA) for a problem with no explicit label.
pub fn problem_letter(position: i32) -> String {
    let mut n = position.max(0) as u64;
    let mut out = Vec::new();
    loop {
        out.push((b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    out.iter().rev().collect()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArbitraryJobRequest {
    #[serde(default)]
    pub contest_id: Option<i32>,
    /// Problem the code was written against, when printed from a problem page.
    /// Combined with `contest_id` to label the printout.
    #[serde(default)]
    pub problem_id: Option<i32>,
    pub filename: String,
    #[serde(default)]
    pub language: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HeartbeatRequest {
    pub station: String,
    #[serde(default)]
    pub printers: Vec<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub queue_seen: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClaimRequest {
    pub station: String,
    #[serde(default)]
    pub printer: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatusRequest {
    pub status: String,
    #[serde(default)]
    pub pages: Option<i32>,
    #[serde(default)]
    pub error: Option<String>,
}

/// A print job without its source body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRow {
    pub id: i64,
    pub contest_id: Option<i32>,
    pub user_id: i32,
    pub username: String,
    pub display_name: Option<String>,
    pub problem_label: Option<String>,
    pub submission_id: Option<i32>,
    pub language: String,
    pub filename: String,
    pub pages_est: Option<i32>,
    pub pages: Option<i32>,
    pub location: Option<String>,
    pub target_printer: Option<String>,
    pub status: String,
    pub claimed_by: Option<String>,
    pub claimed_printer: Option<String>,
    pub error: Option<String>,
    pub created_at: Option<f64>,
    pub claimed_at: Option<f64>,
    pub printed_at: Option<f64>,
}

/// A claimable job handed to a station, source included.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationJob {
    pub id: i64,
    pub contest_id: Option<i32>,
    pub username: String,
    pub display_name: Option<String>,
    pub problem_label: Option<String>,
    pub language: String,
    pub filename: String,
    pub source: String,
    pub location: Option<String>,
    pub target_printer: Option<String>,
    pub created_at: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationRow {
    pub name: String,
    pub location: Option<String>,
    #[serde(default)]
    pub printers: serde_json::Value,
    pub version: Option<String>,
    pub queue_seen: Option<i32>,
    pub last_seen: Option<f64>,
    pub online: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_source_is_one_page() {
        assert_eq!(estimate_pages(""), 1);
        assert_eq!(estimate_pages("\n\n\n"), 1);
    }

    #[test]
    fn short_listing_is_one_page() {
        let src = "int main() {\n    return 0;\n}\n";
        assert_eq!(estimate_pages(src), 1);
    }

    #[test]
    fn long_lines_wrap_into_more_visual_lines() {
        let line = "x".repeat(270);
        assert_eq!(estimate_pages_with(&line, 90, 54), 1);
        let many = vec![line.as_str(); 54].join("\n");
        assert_eq!(estimate_pages_with(&many, 90, 54), 3);
    }

    #[test]
    fn pagination_rounds_up() {
        let src = vec!["line"; 55].join("\n");
        assert_eq!(estimate_pages_with(&src, 90, 54), 2);
    }

    #[test]
    fn problem_letters() {
        assert_eq!(problem_letter(0), "A");
        assert_eq!(problem_letter(25), "Z");
        assert_eq!(problem_letter(26), "AA");
        assert_eq!(problem_letter(27), "AB");
    }

    #[test]
    fn status_validation() {
        assert!(status::is_valid(status::PENDING));
        assert!(status::is_valid(status::DONE));
        assert!(!status::is_valid("bogus"));
        assert!(status::STATION_SETTABLE.contains(&status::PRINTING));
        assert!(!status::STATION_SETTABLE.contains(&status::PENDING));
    }
}
