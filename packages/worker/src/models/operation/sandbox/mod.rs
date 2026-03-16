#![allow(unused_imports)]

pub mod error;
pub mod isolate;
pub mod mock;

use async_trait::async_trait;
pub use broccoli_server_sdk::types::{
    DirectoryOptions, DirectoryRule, EnvRule, ExecutionResult, ResourceLimits, RunOptions,
};
use error::SandboxError;
use std::path::PathBuf;

#[async_trait]
pub trait SandboxManager {
    async fn create_sandbox(&self, id: Option<&str>) -> Result<PathBuf, SandboxError>;
    async fn remove_sandbox(&self, id: &str) -> Result<(), SandboxError>;
    async fn execute(
        &self,
        box_id: &str,
        argv: Vec<String>,
        run_options: &RunOptions,
    ) -> Result<ExecutionResult, SandboxError>;
}
