use clap::Parser;

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

    #[arg(long, default_value_t = 20)]
    pub rate: u32,

    #[arg(long, default_value_t = 50)]
    pub concurrency: u32,

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

        if self.skip_correctness && (self.skip_load || self.correctness_only) {
            return Err(
                "--skip-correctness cannot be combined with --skip-load or --correctness-only; the run would have nothing to do"
                    .to_string(),
            );
        }

        if self.total == 0 {
            return Err("--total must be greater than zero".to_string());
        }
        if self.rate == 0 {
            return Err("--rate must be greater than zero".to_string());
        }
        if self.concurrency == 0 {
            return Err("--concurrency must be greater than zero".to_string());
        }

        Ok(())
    }
}
