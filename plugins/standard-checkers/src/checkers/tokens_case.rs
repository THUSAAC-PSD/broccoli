use broccoli_server_sdk::types::*;

use crate::util::{token_count_msg, token_mismatch_msg, tokenize};

/// Case-insensitive token comparison.
///
/// Uses Unicode case folding (`str::to_lowercase`) so that non-ASCII
/// letters (e.g. "Ü" vs "ü") are compared correctly.
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

    for (i, (exp, act)) in expected.iter().zip(actual.iter()).enumerate() {
        if exp.to_lowercase() != act.to_lowercase() {
            return Ok(CheckerVerdict {
                verdict: Verdict::WrongAnswer,
                score: 0.0,
                // Show original-case tokens in message
                message: Some(token_mismatch_msg(i + 1, exp, act)),
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
    fn yes_vs_yes_case_insensitive() {
        let req = input("YES\n", "yes");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn hello_world_case_insensitive() {
        let req = input("Hello World\n", "hello world");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn different_tokens_regardless_of_case() {
        let req = input("abc\n", "xyz");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::WrongAnswer);
    }

    #[test]
    fn unicode_case_insensitive() {
        let req = input("Ü Ö Ä\n", "ü ö ä");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }
}
