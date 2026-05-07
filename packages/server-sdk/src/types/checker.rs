use serde::{Deserialize, Serialize};

use super::evaluate::JudgeFile;
use super::submission::SourceFile;
use super::verdict::Verdict;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckerParseInput {
    #[serde(default)]
    pub stdout: JudgeFile,
    pub stderr: String,
    pub exit_code: i32,
    #[serde(default)]
    pub expected_output: JudgeFile,
    #[serde(default)]
    pub test_input: JudgeFile,
    #[serde(default)]
    pub checker_source: Option<Vec<SourceFile>>,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckerVerdict {
    pub verdict: Verdict,
    pub score: f64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCheckerInput {
    pub format: String,
    #[serde(flatten)]
    pub input: CheckerParseInput,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileRef, JudgeFile};

    #[test]
    fn checker_parse_input_uses_typed_judge_files() {
        let input = CheckerParseInput {
            stdout: JudgeFile::blob(FileRef {
                filename: "output.txt".to_string(),
                content_type: Some("text/plain".to_string()),
                blob_hash: "stdout-hash".to_string(),
                read_token: None,
            }),
            stderr: "stderr".to_string(),
            exit_code: 0,
            expected_output: JudgeFile::inline("expected\n"),
            test_input: JudgeFile::Missing,
            checker_source: None,
            config: None,
        };

        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["stdout"]["kind"], "blob");
        assert_eq!(json["expected_output"]["kind"], "inline");
        assert_eq!(json["test_input"]["kind"], "missing");
        assert!(json.get("stdout_ref").is_none());
        assert!(json.get("expected_output_ref").is_none());
        assert!(json.get("test_input_ref").is_none());
    }
}
