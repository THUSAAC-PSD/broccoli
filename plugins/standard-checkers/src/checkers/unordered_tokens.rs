use std::collections::HashMap;

use broccoli_server_sdk::types::*;

use crate::util::{token_count_msg, tokenize, truncate};

/// Multiset token comparison — order doesn't matter.
pub fn check(req: &CheckerParseInput) -> Result<CheckerVerdict, String> {
    let expected = tokenize(&req.expected_output);
    let actual = tokenize(&req.stdout);

    if expected.len() != actual.len() {
        return Ok(CheckerVerdict {
            verdict: Verdict::WrongAnswer,
            score: 0.0,
            message: Some(token_count_msg(expected.len(), actual.len())),
        });
    }

    let mut expected_counts: HashMap<&str, usize> = HashMap::new();
    for t in &expected {
        *expected_counts.entry(t).or_default() += 1;
    }

    let mut actual_counts: HashMap<&str, usize> = HashMap::new();
    for t in &actual {
        *actual_counts.entry(t).or_default() += 1;
    }

    // Find first mismatch
    for (token, &exp_count) in &expected_counts {
        let act_count = actual_counts.get(token).copied().unwrap_or(0);
        if act_count != exp_count {
            return Ok(CheckerVerdict {
                verdict: Verdict::WrongAnswer,
                score: 0.0,
                message: Some(format!(
                    "Token '{}': expected {} occurrences, got {}",
                    truncate(token, 50),
                    exp_count,
                    act_count
                )),
            });
        }
    }

    // Check for extra tokens in actual
    for (token, &act_count) in &actual_counts {
        if !expected_counts.contains_key(token) {
            return Ok(CheckerVerdict {
                verdict: Verdict::WrongAnswer,
                score: 0.0,
                message: Some(format!(
                    "Unexpected token '{}' (appeared {} times)",
                    truncate(token, 50),
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
    fn same_tokens_different_order() {
        let req = input("3 1 2", "1 2 3");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn duplicate_multiset_match() {
        let req = input("a a b", "b a a");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn extra_token() {
        let req = input("a b c", "a b");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::WrongAnswer);
    }

    #[test]
    fn missing_token() {
        let req = input("a b", "a b c");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::WrongAnswer);
    }
}
