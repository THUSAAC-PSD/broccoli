use broccoli_server_sdk::types::*;

use crate::util::diff_preview;

/// True byte-exact comparison.
pub fn check(req: &CheckerParseInput) -> Result<CheckerVerdict, String> {
    if req.stdout == req.expected_output {
        Ok(CheckerVerdict {
            verdict: Verdict::Accepted,
            score: 1.0,
            message: None,
        })
    } else {
        Ok(CheckerVerdict {
            verdict: Verdict::WrongAnswer,
            score: 0.0,
            message: Some(diff_preview(&req.expected_output, &req.stdout, 200)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkers::input;

    #[test]
    fn identical_accepted() {
        let req = input("42\n", "42\n");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
        assert_eq!(v.score, 1.0);
    }

    #[test]
    fn trailing_newline_differs() {
        let req = input("42\n", "42");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::WrongAnswer);
    }

    #[test]
    fn empty_both_accepted() {
        let req = input("", "");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }

    #[test]
    fn empty_vs_nonempty() {
        let req = input("", "hello");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::WrongAnswer);
    }
}
