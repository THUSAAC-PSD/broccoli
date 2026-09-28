//! `release/install.sh` runs this binary as its post-install smoke test.
//! Parse that exact invocation with the real CLI, so removing or renaming a
//! flag the installer uses fails here instead of on an operator's machine.

use clap::Parser;
use stress_test::cli::Cli;

const INSTALL_SH: &str = include_str!("../../../release/install.sh");
const COMMAND: &str = "./stress-test/broccoli-stress-test \\";

/// The installer's arguments, with shell expansions replaced by sample values.
fn installer_args() -> Vec<String> {
    let start = INSTALL_SH
        .lines()
        .position(|l| l.trim() == COMMAND)
        .expect("install.sh no longer runs ./stress-test/broccoli-stress-test");
    let mut args = vec!["broccoli-stress-test".to_string()];
    for line in INSTALL_SH.lines().skip(start + 1) {
        let continued = line.trim_end().ends_with('\\');
        let part = line.trim().trim_end_matches('\\').trim();
        let (flag, value) = match part.split_once(' ') {
            Some((flag, value)) => (flag, Some(value.trim())),
            None => (part, None),
        };
        args.push(flag.to_string());
        if let Some(value) = value {
            let sample = if flag == "--url" {
                "http://127.0.0.1:3000"
            } else if value.contains('$') {
                "sample"
            } else {
                value
            };
            args.push(sample.trim_matches('"').to_string());
        }
        if !continued {
            break;
        }
    }
    args
}

#[test]
fn the_installer_smoke_invocation_is_accepted_by_the_cli() {
    let args = installer_args();
    let cli = Cli::try_parse_from(&args)
        .unwrap_or_else(|e| panic!("install.sh passes {args:?}, which the CLI rejects: {e}"));
    cli.validate()
        .unwrap_or_else(|e| panic!("install.sh passes {args:?}, which fails validation: {e}"));
    assert!(
        !cli.skip_load,
        "the smoke test must judge submissions, not only bootstrap: {args:?}"
    );
}
