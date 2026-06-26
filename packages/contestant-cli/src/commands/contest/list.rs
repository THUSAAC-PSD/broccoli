use anyhow::Context;
use chrono::{DateTime, Utc};
use console::Term;
use console::style;

use broccoli_cli_core::client::Client;
use broccoli_cli_core::config;

pub fn run() -> anyhow::Result<()> {
    let creds = config::resolve_credentials(None, None)?;
    let client = Client::new(creds);

    println!("{}  Fetching contests...", style("→").blue().bold());

    let resp = client
        .list_contests()
        .context("Failed to fetch contest list")?;

    if resp.data.is_empty() {
        println!("  No contests available.");
        return Ok(());
    }

    let term_width = Term::stdout().size().1 as usize;

    if term_width >= 80 {
        print_table(&resp.data);
    } else {
        print_cards(&resp.data);
    }

    Ok(())
}

fn status_label(start_time: &str, end_time: &str) -> (String, console::StyledObject<&'static str>) {
    let now = Utc::now();

    let start = DateTime::parse_from_rfc3339(start_time)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or(now);
    let end = DateTime::parse_from_rfc3339(end_time)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or(now);

    if now < start {
        ("Upcoming".into(), style("Upcoming").cyan())
    } else if now > end {
        ("Finished".into(), style("Finished").dim())
    } else {
        ("Running".into(), style("Running").green().bold())
    }
}

fn format_time(ts: &str) -> String {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| ts.to_string())
}

fn print_table(contests: &[broccoli_cli_core::client::ContestListItem]) {
    println!(
        "  {:<6} {:<40} {:<12} {:<20} {:<20}",
        style("ID").bold(),
        style("Name").bold(),
        style("Status").bold(),
        style("Start").bold(),
        style("End").bold(),
    );
    println!("  {}", style("-".repeat(98)).dim());

    for c in contests {
        let (_status_str, styled_status) = status_label(&c.start_time, &c.end_time);
        println!(
            "  {:<6} {:<40} {}   {:<20} {:<20}",
            style(&c.id).yellow(),
            truncate(&c.title, 38),
            styled_status,
            format_time(&c.start_time),
            format_time(&c.end_time),
        );
    }
}

fn print_cards(contests: &[broccoli_cli_core::client::ContestListItem]) {
    for (i, c) in contests.iter().enumerate() {
        let (_status_str, styled_status) = status_label(&c.start_time, &c.end_time);
        println!("  {} {}", style("#").dim(), i + 1);
        println!("    ID:     {}", style(&c.id).yellow());
        println!("    Name:   {}", c.title);
        println!("    Status: {}", styled_status);
        println!("    Start:  {}", format_time(&c.start_time));
        println!("    End:    {}", format_time(&c.end_time));
        println!();
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
