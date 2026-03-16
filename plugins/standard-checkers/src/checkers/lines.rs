use broccoli_server_sdk::types::*;

use crate::util::{line_count_msg, line_mismatch_msg, split_lines_trimmed};

/// Per-line comparison with trailing whitespace normalization.
///
/// Trailing empty lines are ignored from both sides.
pub fn check(req: &CheckerParseInput) -> Result<CheckerVerdict, String> {
    let expected = split_lines_trimmed(&req.expected_output);
    let actual = split_lines_trimmed(&req.stdout);

    if expected.len() != actual.len() {
        return Ok(CheckerVerdict {
            verdict: Verdict::WrongAnswer,
            score: 0.0,
            message: Some(line_count_msg(expected.len(), actual.len())),
        });
    }

    for (i, (exp, act)) in expected.iter().zip(actual.iter()).enumerate() {
        if exp != act {
            return Ok(CheckerVerdict {
                verdict: Verdict::WrongAnswer,
                score: 0.0,
                message: Some(line_mismatch_msg(i + 1, exp, act)),
            });
        }
    }

    Ok(CheckerVerdict {
        verdict: Verdict::Accepted,
        score: 1.0,
        message: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkers::input;

    #[test]
    fn trailing_whitespace_per_line_ignored() {
        let req = input("hello  \nworld  \n", "hello\nworld\n");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn trailing_empty_lines_ignored() {
        let req = input("hello\n\n\n", "hello\n");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn internal_whitespace_preserved() {
        let req = input("hello  world\n", "hello world\n");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::WrongAnswer);
    }

    #[test]
    fn different_line_count() {
        let req = input("a\nb\n", "a\nb\nc\n");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::WrongAnswer);
        assert!(v.message.unwrap().contains("Line count mismatch"));
    }

    #[test]
    fn first_diff_line_reported() {
        let req = input("a\nX\nc\n", "a\nb\nc\n");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::WrongAnswer);
        assert!(v.message.unwrap().contains("Line 2"));
    }
}
