pub mod clarifications;
pub mod config;
pub mod contest;
pub mod context;
pub mod login;
pub mod prewarm;
pub mod status;
pub mod submit;
pub mod test;
pub mod watch;
pub mod whoami;

use self::clarifications::ClarificationsArgs;
use self::config::ConfigArgs;
use self::contest::ContestArgs;
use self::login::LoginArgs;
use self::status::StatusArgs;
use self::submit::SubmitArgs;
use self::test::TestArgs;
use self::watch::WatchArgs;
use clap::{Args, Subcommand};

// `status` never gets `s` so a fat-finger can't turn a status check into a submit.
#[derive(Subcommand)]
pub enum Command {
    /// Log in to a Broccoli contest server
    #[command(alias = "li")]
    Login(LoginArgs),

    /// Show who you're logged in as
    #[command(visible_alias = "me")]
    Whoami,

    /// Submit a solution to a problem
    #[command(visible_alias = "s", alias = "sub")]
    Submit(SubmitArgs),

    /// Test a solution against sample cases (remote or local)
    #[command(visible_alias = "t", alias = "tst")]
    Test(TestArgs),

    /// Manage contests (list, info, register, unregister, problems)
    #[command(visible_alias = "c", alias = "con")]
    Contest(ContestArgs),

    /// Query submission status (interactive picker when no id is given)
    #[command(visible_alias = "st", aliases = ["ss", "stat"])]
    Status(StatusArgs),

    /// List or ask clarifications for a contest
    #[command(visible_alias = "clar", aliases = ["cl", "clarification"])]
    Clarifications(ClarificationsArgs),

    /// Show or modify CLI configuration
    #[command(visible_alias = "cfg", alias = "conf")]
    Config(ConfigArgs),

    /// Watch a contest in real-time (TUI dashboard)
    #[command(visible_alias = "w", alias = "dash")]
    Watch(WatchArgs),

    /// Generate a shell completion script (bash, zsh, fish, powershell, elvish)
    Completions(CompletionsArgs),

    /// Warm DNS/TLS + the binary cache so the first real command is snappy
    Prewarm,
}

#[derive(Args)]
pub struct CompletionsArgs {
    /// Shell to generate a completion script for
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}
