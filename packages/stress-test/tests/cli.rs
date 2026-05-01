use clap::Parser;
use stress_test::cli::Cli;

#[test]
fn parses_minimal_url_plus_token() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url",
        "http://localhost:3000",
        "--admin-token",
        "abc",
    ])
    .unwrap();
    assert_eq!(cli.url, "http://localhost:3000");
    assert_eq!(cli.admin_token.as_deref(), Some("abc"));
    assert_eq!(cli.total, 200);
    assert_eq!(cli.rate, 20);
    assert_eq!(cli.concurrency, 50);
}

#[test]
fn requires_url() {
    let r = Cli::try_parse_from(["broccoli-stress-test", "--admin-token", "abc"]);
    assert!(r.is_err());
}

#[test]
fn rejects_token_and_username_password_both_missing() {
    let cli =
        Cli::try_parse_from(["broccoli-stress-test", "--url", "http://localhost:3000"]).unwrap();
    assert!(cli.validate().is_err());
}

#[test]
fn accepts_username_password_pair() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url",
        "http://localhost:3000",
        "--admin-username",
        "admin",
        "--admin-password",
        "secret",
    ])
    .unwrap();
    assert!(cli.validate().is_ok());
    assert_eq!(cli.admin_username.as_deref(), Some("admin"));
    assert_eq!(cli.admin_password.as_deref(), Some("secret"));
}

#[test]
fn rejects_username_without_password() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url",
        "http://localhost:3000",
        "--admin-username",
        "admin",
    ])
    .unwrap();
    assert!(cli.validate().is_err());
}

#[test]
fn rejects_password_without_username() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url",
        "http://localhost:3000",
        "--admin-password",
        "secret",
    ])
    .unwrap();
    assert!(cli.validate().is_err());
}

#[test]
fn rejects_skip_correctness_and_skip_load_both_set() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url",
        "http://localhost:3000",
        "--admin-token",
        "abc",
        "--skip-correctness",
        "--skip-load",
    ])
    .unwrap();
    assert!(cli.validate().is_err());
}

#[test]
fn rejects_zero_total() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url",
        "http://localhost:3000",
        "--admin-token",
        "abc",
        "--total",
        "0",
    ])
    .unwrap();
    assert!(cli.validate().is_err());
}

#[test]
fn rejects_zero_rate() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url",
        "http://localhost:3000",
        "--admin-token",
        "abc",
        "--rate",
        "0",
    ])
    .unwrap();
    assert!(cli.validate().is_err());
}

#[test]
fn rejects_zero_concurrency() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url",
        "http://localhost:3000",
        "--admin-token",
        "abc",
        "--concurrency",
        "0",
    ])
    .unwrap();
    assert!(cli.validate().is_err());
}

#[test]
fn parses_all_numeric_flags_with_non_default_values() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url",
        "http://localhost:3000",
        "--admin-token",
        "abc",
        "--total",
        "1000",
        "--rate",
        "100",
        "--concurrency",
        "200",
        "--per-job-timeout",
        "120",
        "--p95-budget-ms",
        "30000",
        "--contest-concurrency",
        "40",
        "--seed",
        "42",
    ])
    .unwrap();
    assert_eq!(cli.total, 1000);
    assert_eq!(cli.rate, 100);
    assert_eq!(cli.concurrency, 200);
    assert_eq!(cli.per_job_timeout, 120);
    assert_eq!(cli.p95_budget_ms, 30000);
    assert_eq!(cli.contest_concurrency, 40);
    assert_eq!(cli.seed, 42);
    assert!(cli.validate().is_ok());
}

#[test]
fn parses_contest_id_passthrough_flags() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url",
        "http://localhost:3000",
        "--admin-token",
        "abc",
        "--contest-id",
        "7",
        "--problem-id",
        "13",
    ])
    .unwrap();
    assert_eq!(cli.contest_id, Some(7));
    assert_eq!(cli.problem_id, Some(13));
}

#[test]
fn parses_json_flag() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url",
        "http://localhost:3000",
        "--admin-token",
        "abc",
        "--json",
    ])
    .unwrap();
    assert!(cli.json);
}

#[test]
fn parses_keep_fixtures_flag() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url",
        "http://localhost:3000",
        "--admin-token",
        "abc",
        "--keep-fixtures",
    ])
    .unwrap();
    assert!(cli.keep_fixtures);
}

#[test]
fn defaults_match_design_doc() {
    let cli = Cli::try_parse_from([
        "broccoli-stress-test",
        "--url",
        "http://localhost:3000",
        "--admin-token",
        "abc",
    ])
    .unwrap();
    assert_eq!(cli.total, 200);
    assert_eq!(cli.rate, 20);
    assert_eq!(cli.concurrency, 50);
    assert_eq!(cli.per_job_timeout, 60);
    assert_eq!(cli.p95_budget_ms, 15000);
    assert_eq!(cli.contest_concurrency, 20);
    assert_eq!(cli.seed, 0);
    assert!(!cli.skip_correctness);
    assert!(!cli.skip_load);
    assert!(!cli.keep_fixtures);
    assert!(!cli.json);
    assert!(cli.contest_id.is_none());
    assert!(cli.problem_id.is_none());
}
