// This is temporary models for demonstration purposes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Submission {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JudgeResult {
    pub greeting: String,
}
