use broccoli_server_sdk::types::*;

/// Trivial checker that always returns Accepted with score 1.0.
///
/// Used for "run code" test cases that have no expected output and
/// the user just wants to see what the program produces.
pub fn check(_req: &CheckerParseInput) -> Result<CheckerVerdict, String> {
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
    fn always_accepted() {
        let req = input("any output", "anything");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
        assert_eq!(v.score, 1.0);
    }

    #[test]
    fn empty_output_accepted() {
        let req = input("", "");
        let v = check(&req).unwrap();
        assert_eq!(v.verdict, Verdict::Accepted);
    }
}
