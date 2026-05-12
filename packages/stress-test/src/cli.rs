use clap::{Parser, ValueEnum};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum LoadProfile {
    /// Existing judge-throughput profile: submit official solutions and poll them.
    Judge,
    /// Mixed contest-traffic profile: page reads, scoreboard polling, code-runs, and submissions.
    Mixed,
}

#[derive(Parser, Debug)]
#[command(
    name = "broccoli-stress-test",
    version,
    about = "Broccoli platform stress test",
    long_about = None,
    after_help = "First time? Get the matching binary at <your-server>/downloads.",
)]
pub struct Cli {
    #[arg(long)]
    pub url: String,

    #[arg(long)]
    pub admin_token: Option<String>,

    #[arg(long)]
    pub admin_username: Option<String>,

    #[arg(long)]
    pub admin_password: Option<String>,

    #[arg(long, default_value_t = 200)]
    pub total: u64,

    /// Run duration in seconds. For mixed profile this overrides --total.
    #[arg(long)]
    pub duration: Option<u64>,

    #[arg(long, default_value_t = 20)]
    pub rate: u32,

    #[arg(long, default_value_t = 50)]
    pub concurrency: u32,

    #[arg(long, value_enum, default_value_t = LoadProfile::Judge)]
    pub profile: LoadProfile,

    /// Number of contestant accounts to create/enroll for mixed profile traffic.
    #[arg(long, default_value_t = 0)]
    pub contestants: u32,

    #[arg(long, default_value_t = 60)]
    pub per_job_timeout: u64,

    #[arg(long, default_value_t = 15000)]
    pub p95_budget_ms: u64,

    #[arg(long)]
    pub contest_type: Option<String>,

    #[arg(long)]
    pub problem_type: Option<String>,

    #[arg(long)]
    pub contest_id: Option<i32>,

    #[arg(long)]
    pub problem_id: Option<i32>,

    #[arg(long, default_value_t = 20)]
    pub contest_concurrency: u32,

    #[arg(long, default_value_t = false)]
    pub skip_correctness: bool,

    #[arg(long, default_value_t = false)]
    pub skip_load: bool,

    /// Run only the correctness phase. Alias for `--skip-load`.
    #[arg(long, default_value_t = false)]
    pub correctness_only: bool,

    #[arg(long, default_value_t = false)]
    pub keep_fixtures: bool,

    #[arg(long, default_value_t = 0)]
    pub seed: u64,

    #[arg(long, default_value_t = false)]
    pub json: bool,

    /// Skip the startup version handshake against the server.
    #[arg(long, default_value_t = false)]
    pub no_version_check: bool,

    /// Stable identifier attached to every request for log/trace correlation.
    #[arg(long)]
    pub run_id: Option<String>,

    /// Duration, in seconds, for the final burst window in the mixed profile.
    #[arg(long, default_value_t = 0)]
    pub final_burst_duration: u64,

    /// Rate multiplier used during the final burst window in the mixed profile.
    #[arg(long, default_value_t = 3)]
    pub final_burst_multiplier: u32,
}

impl Cli {
    pub fn validate(&self) -> Result<(), String> {
        let has_token = self.admin_token.is_some();
        let has_user_pass = self.admin_username.is_some() && self.admin_password.is_some();
        if !has_token && !has_user_pass {
            return Err(
                "must provide --admin-token, or both --admin-username and --admin-password"
                    .to_string(),
            );
        }

        if self.correctness_only {
            return Err(
                "--correctness-only is no longer supported; the stress test now runs load against real contest data, or bootstraps A+B fixtures when no --contest-id is provided"
                    .to_string(),
            );
        }

        if self.total == 0 {
            return Err("--total must be greater than zero".to_string());
        }
        if let Some(duration) = self.duration
            && duration == 0
        {
            return Err("--duration must be greater than zero".to_string());
        }
        if self.rate == 0 {
            return Err("--rate must be greater than zero".to_string());
        }
        if self.concurrency == 0 {
            return Err("--concurrency must be greater than zero".to_string());
        }
        if let Some(run_id) = &self.run_id
            && (run_id.is_empty()
                || !run_id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'))
        {
            return Err(
                "--run-id must contain only ASCII alphanumeric characters, '.', '-', or '_'"
                    .to_string(),
            );
        }
        if self.final_burst_duration > 0 && self.final_burst_multiplier == 0 {
            return Err(
                "--final-burst-multiplier must be greater than zero when burst is enabled"
                    .to_string(),
            );
        }

        Ok(())
    }
}
