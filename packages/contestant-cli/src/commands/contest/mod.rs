pub mod info;
pub mod list;
pub mod problems;
pub mod register;

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct ContestArgs {
    #[command(subcommand)]
    pub command: ContestCommand,
}

#[derive(Subcommand)]
pub enum ContestCommand {
    /// List available contests
    #[command(visible_alias = "ls")]
    List,
    /// Show contest details and your registration status
    #[command(visible_alias = "i", alias = "show")]
    Info(InfoArgs),
    /// Register for a contest
    #[command(visible_alias = "reg")]
    Register(RegisterArgs),
    /// Unregister from a contest
    #[command(visible_alias = "unreg")]
    Unregister(RegisterArgs),
    /// List or download contest problems
    #[command(visible_alias = "p", alias = "probs")]
    Problems(ProblemsArgs),
}

pub use info::InfoArgs;
pub use problems::ProblemsArgs;
pub use register::RegisterArgs;
