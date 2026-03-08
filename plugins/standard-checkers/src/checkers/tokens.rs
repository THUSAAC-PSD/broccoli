use broccoli_server_sdk::types::*;

use crate::util::{token_count_msg, token_mismatch_msg, tokenize};

/// Whitespace-insensitive token comparison.
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
        if exp != act {
            return Ok(CheckerVerdict {
                verdict: Verdict::WrongAnswer,
                score: 0.0,
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
    fn different_whitespace_same_tokens() {
        let req = input("  1   2  3  \n", "1 2 3");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn tokens_on_different_lines() {
        let req = input("1\n2\n3\n", "1 2 3");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn token_count_mismatch() {
        let req = input("1 2", "1 2 3");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::WrongAnswer);
        assert!(v.message.unwrap().contains("Token count mismatch"));
    }

    #[test]
    fn token_value_mismatch() {
        let req = input("1 X 3", "1 2 3");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::WrongAnswer);
        assert!(v.message.unwrap().contains("position 2"));
    }

    #[test]
    fn empty_vs_nonempty() {
        let req = input("", "hello");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::WrongAnswer);
    }
}
