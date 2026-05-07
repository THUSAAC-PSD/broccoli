use std::borrow::Cow;

use super::verdict::Verdict;

pub const RESULT_TEXT_DB_LIMIT_CHARS: usize = 64 * 1024;

pub fn sanitize_text_field(s: &str) -> Cow<'_, str> {
    if s.contains('\0') {
        Cow::Owned(s.replace('\0', "\u{FFFD}"))
    } else {
        Cow::Borrowed(s)
    }
}

pub fn sanitize_result_text_field(s: &str) -> Cow<'_, str> {
    let sanitized = sanitize_text_field(s);
    if sanitized.chars().count() <= RESULT_TEXT_DB_LIMIT_CHARS {
        return sanitized;
    }

    let mut truncated: String = sanitized.chars().take(RESULT_TEXT_DB_LIMIT_CHARS).collect();
    truncated.push_str("\n... (truncated)");
    Cow::Owned(truncated)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionStatus {
    Running,
    Judged,
    CompilationError,
}

impl SubmissionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Judged => "Judged",
            Self::CompilationError => "CompilationError",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Judged | Self::CompilationError)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubmissionUpdate {
    pub submission_id: i32,
    /// Targeted `submission_judgement` row. The SDK writes the result
    /// fields to this judgement and mirrors the cache columns onto the
    /// owning submission row in the same statement. A non-positive value
    /// means the caller is on a legacy path that does not know about
    /// judgements; the SDK skips the judgement update in that case.
    pub judgement_id: i32,
    pub judge_epoch: i32,
    pub status: Option<SubmissionStatus>,
    pub verdict: Option<Option<Verdict>>,
    pub score: Option<f64>,
    pub time_used: Option<Option<i32>>,
    pub memory_used: Option<Option<i32>>,
    pub compile_output: Option<Option<String>>,
    pub error_code: Option<Option<String>>,
    pub error_message: Option<Option<String>>,
}

impl SubmissionUpdate {
    pub fn new(submission_id: i32, judgement_id: i32, judge_epoch: i32) -> Self {
        Self {
            submission_id,
            judgement_id,
            judge_epoch,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestCaseResultRow {
    pub submission_id: i32,
    /// Targeted `submission_judgement` row. The SDK writes this onto
    /// `test_case_result.judgement_id`. Zero or negative means a legacy
    /// caller that does not know about judgements; the column stays
    /// NULL until `seed::backfill_submission_judgements` wires it up
    /// on the next server boot.
    pub judgement_id: i32,
    pub test_case_id: Option<i32>,
    pub run_index: Option<i32>,
    pub verdict: Verdict,
    pub score: f64,
    pub time_used: Option<i32>,
    pub memory_used: Option<i32>,
    pub message: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CodeRunUpdate {
    pub code_run_id: i32,
    pub status: Option<SubmissionStatus>,
    pub verdict: Option<Option<Verdict>>,
    pub score: Option<f64>,
    pub time_used: Option<Option<i32>>,
    pub memory_used: Option<Option<i32>>,
    pub compile_output: Option<Option<String>>,
    pub error_code: Option<Option<String>>,
    pub error_message: Option<Option<String>>,
}

impl CodeRunUpdate {
    pub fn new(code_run_id: i32) -> Self {
        Self {
            code_run_id,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodeRunResultRow {
    pub code_run_id: i32,
    pub run_index: i32,
    pub verdict: Verdict,
    pub score: f64,
    pub time_used: Option<i32>,
    pub memory_used: Option<i32>,
    pub message: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_text_field_passes_through_clean_strings() {
        let s = "hello world\nline2";
        let out = sanitize_text_field(s);
        assert!(matches!(out, Cow::Borrowed(_)));
        assert_eq!(out, "hello world\nline2");
    }

    #[test]
    fn sanitize_text_field_replaces_nul_with_replacement_char() {
        let s = "abc\0def\0\0xyz";
        let out = sanitize_text_field(s);
        assert!(matches!(out, Cow::Owned(_)));
        assert_eq!(out, "abc\u{FFFD}def\u{FFFD}\u{FFFD}xyz");
        // 1 NUL byte → 1 replacement char (not stripped), so char count is preserved.
        assert_eq!(out.chars().count(), s.chars().count());
    }

    #[test]
    fn sanitize_text_field_handles_empty_and_only_nul() {
        assert_eq!(sanitize_text_field(""), "");
        assert_eq!(sanitize_text_field("\0"), "\u{FFFD}");
        assert_eq!(sanitize_text_field("\0\0\0"), "\u{FFFD}\u{FFFD}\u{FFFD}");
    }

    #[test]
    fn sanitize_result_text_field_truncates_large_values() {
        let input = "a".repeat(RESULT_TEXT_DB_LIMIT_CHARS + 10);
        let out = sanitize_result_text_field(&input);
        assert!(matches!(out, Cow::Owned(_)));
        assert_eq!(
            out.as_ref(),
            format!(
                "{}\n... (truncated)",
                "a".repeat(RESULT_TEXT_DB_LIMIT_CHARS)
            )
        );
    }
}
