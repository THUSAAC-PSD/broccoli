use super::context;
use anyhow::Context;
use broccoli_cli_core::client::{Client, SubmissionFileDto};
use broccoli_cli_core::config::{self, load_user_config};
use broccoli_cli_core::resolve;
use clap::Args;
use console::style;
use std::fs;
use std::time::Duration;

#[derive(Args)]
pub struct SubmitArgs {
    /// Source files to submit
    #[arg(required = true)]
    pub files: Vec<String>,

    /// Contest ID or name (e.g. 3 or "Spring Round")
    #[arg(short = 'c', long, conflicts_with = "no_contest")]
    pub contest: Option<String>,

    /// Problem ID, label, index, or title (e.g. A, 1, or "Two Sum")
    #[arg(short = 'p', long)]
    pub problem: Option<String>,

    /// Submit to a standalone problem instead of a contest
    #[arg(long, default_value_t = false)]
    pub no_contest: bool,

    /// Programming language (auto-detected from file extension if omitted)
    #[arg(short = 'l', long)]
    pub language: Option<String>,

    /// Watch for the verdict after submitting
    #[arg(short = 'w', long, default_value_t = false)]
    pub watch: bool,
}

pub fn run(args: SubmitArgs) -> anyhow::Result<()> {
    let creds = config::resolve_credentials(None, None)?;
    let client = Client::new(creds);
    let user_config = load_user_config();
    let ctx = context::discover_context();

    let problem_ref = args
        .problem
        .as_deref()
        .or(ctx.as_ref().and_then(|c| c.problem.as_deref()))
        .context("No problem specified. Use -p <id|label> (e.g. -p A).")?;

    let contest_id: Option<String> = if args.no_contest {
        None
    } else {
        let contest_ref = args
            .contest
            .as_deref()
            .or(ctx.as_ref().and_then(|c| c.contest.as_deref()))
            .or(user_config.contest.as_deref())
            .context(
                "No contest specified. Use -c <id|name>, or --no-contest to submit \
                 to a standalone problem.",
            )?;
        Some(resolve::contest_id(&client, contest_ref)?)
    };
    let problem_id: String = match contest_id.as_deref() {
        Some(cid) => resolve::problem_id(&client, cid, problem_ref)?,
        // standalone: no contest to resolve labels against, expect a raw id
        None => problem_ref.to_string(),
    };

    let mut submission_files = Vec::new();
    for path in &args.files {
        let content =
            fs::read_to_string(path).with_context(|| format!("Failed to read: {}", path))?;
        let filename = std::path::Path::new(path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        submission_files.push(SubmissionFileDto { filename, content });
    }

    let language = args
        .language
        .as_deref()
        .or_else(|| args.files.first().and_then(|f| context::detect_language(f)))
        .unwrap_or("cpp")
        .to_string();

    println!(
        "{}  Language: {}",
        style("→").blue().bold(),
        style(&language).cyan()
    );
    match contest_id.as_deref() {
        Some(c) => println!(
            "{}  Submitting to contest {}...",
            style("→").blue().bold(),
            style(c).cyan()
        ),
        None => println!(
            "{}  Submitting to standalone problem {}...",
            style("→").blue().bold(),
            style(&problem_id).cyan()
        ),
    }

    let submission = match contest_id.as_deref() {
        Some(cid) => client
            .create_contest_submission(cid, &problem_id, submission_files, &language, None)
            .context("Failed to create submission")?,
        None => client
            .create_submission(&problem_id, submission_files, &language, None)
            .context("Failed to create submission")?,
    };

    println!(
        "{}  Submitted! ID: {}",
        style("✓").green().bold(),
        style(submission.id).yellow()
    );

    // persist resolved ids so future runs skip the name/label lookup
    if ctx.is_none() {
        let _ = context::save_context(&context::ProjectContext {
            contest: contest_id.clone(),
            problem: Some(problem_id.clone()),
            language: Some(language),
        });
    }

    if args.watch {
        watch_verdict(&client, submission.id)?;
    } else {
        println!(
            "   Check the verdict any time with: {}",
            style(format!("broccoli status {}", submission.id)).cyan()
        );
    }

    Ok(())
}

fn watch_verdict(client: &Client, submission_id: i32) -> anyhow::Result<()> {
    use std::io::Write;
    let submission_id = submission_id.to_string();
    let submission_id = submission_id.as_str();
    let mut spinner = broccoli_cli_core::tui::spinner::Spinner::new();
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > Duration::from_secs(300) {
            eprint!("\r\x1b[K");
            std::io::stderr().flush().ok();
            anyhow::bail!(
                "Timed out waiting for the verdict. Check later with `broccoli status {}`.",
                submission_id
            );
        }

        let sub = client
            .get_submission(submission_id)
            .context("Failed to poll submission status")?;
        let elapsed = start.elapsed();

        if sub.status.is_in_progress() {
            // \x1b[K clears stale chars from a previous longer frame
            eprint!(
                "\r\x1b[K  {} Judging... {} ({:.1}s)",
                spinner.next(),
                sub.status,
                elapsed.as_secs_f64()
            );
            std::io::stderr().flush().ok();
            std::thread::sleep(Duration::from_millis(200));
        } else {
            eprint!("\r\x1b[K");
            std::io::stderr().flush().ok();

            let accepted = sub
                .result
                .as_ref()
                .and_then(|r| r.verdict.as_ref())
                .is_some_and(|v| v.is_accepted());
            // judged verdict if present, else lifecycle status
            let label = match sub.result.as_ref().and_then(|r| r.verdict.as_ref()) {
                Some(v) => v.to_string(),
                None => sub.status.to_string(),
            };
            let score = sub.result.as_ref().and_then(|r| r.score);

            {
                if accepted {
                    if let Some(s) = score {
                        println!(
                            "{}  Accepted — {}/100 points! ({:.1}s)",
                            style("✓").green().bold(),
                            s,
                            elapsed.as_secs_f64()
                        );
                    } else {
                        println!(
                            "{}  Accepted! ({:.1}s)",
                            style("✓").green().bold(),
                            elapsed.as_secs_f64()
                        );
                    }
                } else {
                    let sym = style("✗").red().bold();
                    if let Some(s) = score {
                        println!(
                            "{}  {} — {}/100 points ({:.1}s)",
                            sym,
                            label,
                            s,
                            elapsed.as_secs_f64()
                        );
                    } else {
                        println!("{}  {} ({:.1}s)", sym, label, elapsed.as_secs_f64());
                    }
                }
            }

            if let Some(ref result) = sub.result {
                if let Some(msg) = result.error_message.as_deref().filter(|m| !m.is_empty()) {
                    println!("  {}", style("Error:").red().bold());
                    for line in msg.lines() {
                        println!("    {}", style(line).red());
                    }
                }
                if let Some(ref co) = result.compile_output {
                    if !co.is_empty() {
                        println!("\n  Compilation output:");
                        for line in co.lines() {
                            println!("    {}", style(line).dim());
                        }
                    }
                }
            }

            return Ok(());
        }
    }
}
