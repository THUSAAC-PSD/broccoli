pub mod exact;
pub mod lines;
pub mod testlib;
pub mod tokens;
pub mod tokens_case;
pub mod tokens_float;
pub mod unordered_lines;
pub mod unordered_tokens;

#[cfg(test)]
use broccoli_server_sdk::types::*;

#[cfg(test)]
pub fn input(stdout: &str, expected: &str) -> CheckerParseInput {
    CheckerParseInput {
        stdout: stdout.into(),
        stderr: String::new(),
        exit_code: 0,
        expected_output: expected.into(),
        test_input: String::new(),
        checker_source: None,
        config: None,
    }
}

#[cfg(test)]
pub fn input_with_config(
    stdout: &str,
    expected: &str,
    config: serde_json::Value,
) -> CheckerParseInput {
    CheckerParseInput {
        stdout: stdout.into(),
        stderr: String::new(),
        exit_code: 0,
        expected_output: expected.into(),
        test_input: String::new(),
        checker_source: None,
        config: Some(config),
    }
}
