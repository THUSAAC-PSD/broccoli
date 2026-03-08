use std::collections::HashMap;

use broccoli_server_sdk::types::*;

use crate::util::{line_count_msg, split_lines_trimmed, truncate};

/// Multiset line comparison — order doesn't matter.
/// Trailing whitespace per line is ignored. Trailing empty lines are dropped.
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

    let mut expected_counts: HashMap<&str, usize> = HashMap::new();
    for l in &expected {
        *expected_counts.entry(l).or_default() += 1;
    }

    let mut actual_counts: HashMap<&str, usize> = HashMap::new();
    for l in &actual {
        *actual_counts.entry(l).or_default() += 1;
    }

    for (line, &exp_count) in &expected_counts {
        let act_count = actual_counts.get(line).copied().unwrap_or(0);
        if act_count != exp_count {
            return Ok(CheckerVerdict {
                verdict: Verdict::WrongAnswer,
                score: 0.0,
                message: Some(format!(
                    "Line '{}': expected {} occurrences, got {}",
                    truncate(line, 100),
                    exp_count,
                    act_count
                )),
            });
        }
    }

    for (line, &act_count) in &actual_counts {
        if !expected_counts.contains_key(line) {
            return Ok(CheckerVerdict {
                verdict: Verdict::WrongAnswer,
                score: 0.0,
                message: Some(format!(
                    "Unexpected line '{}' (appeared {} times)",
                    truncate(line, 100),
                    act_count
                )),
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
    fn same_lines_different_order() {
        let req = input("b\na\nc\n", "a\nb\nc\n");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn trailing_whitespace_ignored() {
        let req = input("a  \nb  \n", "b\na\n");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn duplicate_lines_multiset() {
        let req = input("a\na\nb\n", "b\na\na\n");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn different_lines() {
        let req = input("a\nb\n", "a\nc\n");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::WrongAnswer);
    }
}
