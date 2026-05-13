pub mod cancel_storm;

use async_trait::async_trait;

use crate::transcript::Transcript;

pub struct ScenarioContext {
    pub redis_url: String,
}

#[derive(Debug)]
pub struct ScenarioOutcome {
    pub passed: bool,
    pub transcript: Transcript,
}

#[async_trait]
pub trait Scenario {
    fn name(&self) -> &'static str;
    async fn run(&self, ctx: &ScenarioContext) -> anyhow::Result<ScenarioOutcome>;
}
