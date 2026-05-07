pub mod exact;
pub mod lines;
pub mod none;
pub mod testlib;
pub mod tokens;
pub mod tokens_case;
pub mod tokens_float;

#[cfg(test)]
use broccoli_server_sdk::types::*;

#[cfg(test)]
pub fn input(stdout: &str, expected: &str) -> CheckerParseInput {
    CheckerParseInput {
        stdout: JudgeFile::inline(stdout),
        stderr: String::new(),
        exit_code: 0,
        expected_output: JudgeFile::inline(expected),
        test_input: JudgeFile::Missing,
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
        stdout: JudgeFile::inline(stdout),
        stderr: String::new(),
        exit_code: 0,
        expected_output: JudgeFile::inline(expected),
        test_input: JudgeFile::Missing,
        checker_source: None,
        config: Some(config),
    }
}
