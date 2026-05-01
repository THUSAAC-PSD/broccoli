
use std::fmt::Write as _;
use std::time::Duration;

use crate::load::LoadOutcome;

#[derive(Debug, Default)]
pub struct RunSummary {
    pub target_url: String,
    pub duration: Duration,
    pub bootstrap_error: Option<String>,
    pub correctness: Option<CorrectnessSummary>,
    pub load: Option<LoadSummary>,
    pub passthrough: PassthroughSummary,
    pub cleanup_warnings: Vec<String>,
}

#[derive(Debug)]
pub struct CorrectnessSummary {
    pub total: usize,
    pub passed: usize,
    pub failed_scenarios: Vec<String>,
}

#[derive(Debug)]
pub struct LoadSummary {
    pub total: u64,
    pub completed: u64,
    pub passed: u64,
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub p99_ms: u64,
    pub max_ms: u64,
    pub p95_budget_ms: u64,
    pub passed_budget: bool,
    pub error_count: usize,
    pub passed_overall: bool,
}

impl LoadSummary {
    pub fn from_outcome(outcome: &LoadOutcome, total: u64, p95_budget_ms: u64) -> Self {
        Self {
            total,
            completed: outcome.completed,
            passed: outcome.passed,
            p50_ms: outcome.histogram.value_at_quantile(0.50),
            p95_ms: outcome.histogram.value_at_quantile(0.95),
            p99_ms: outcome.histogram.value_at_quantile(0.99),
            max_ms: outcome.histogram.max(),
            p95_budget_ms,
            passed_budget: outcome.passed_budget,
            error_count: outcome.errors.len(),
            passed_overall: outcome.passed_overall,
        }
    }
}

#[derive(Debug, Default)]
pub enum PassthroughSummary {
    #[default]
    NotRun,
    Skipped {
        reason: String,
    },
    Completed {
        ok: bool,
        count: usize,
    },
}

impl RunSummary {
    pub fn passed(&self) -> bool {
        if self.bootstrap_error.is_some() {
            return false;
        }
        let correctness_ok = self
            .correctness
            .as_ref()
            .is_none_or(|c| c.failed_scenarios.is_empty());
        let load_ok = self.load.as_ref().is_none_or(|l| l.passed_overall);
        let passthrough_ok = matches!(
            self.passthrough,
            PassthroughSummary::NotRun
                | PassthroughSummary::Skipped { .. }
                | PassthroughSummary::Completed { ok: true, .. }
        );
        correctness_ok && load_ok && passthrough_ok
    }
}

pub fn format_summary(summary: &RunSummary) -> String {
    let mut out = String::new();
    let result = if summary.passed() { "PASS" } else { "FAIL" };

    let _ = writeln!(
        out,
        "─────────────────────────────────────────────────────────"
    );
    let _ = writeln!(out, " BROCCOLI STRESS TEST — RESULT: {result}");
    let _ = writeln!(
        out,
        "─────────────────────────────────────────────────────────"
    );
    let _ = writeln!(out, " Target           {}", summary.target_url);
    let _ = writeln!(
        out,
        " Duration         {:.1}s",
        summary.duration.as_secs_f64()
    );

    match &summary.correctness {
        Some(c) => {
            let label = if c.failed_scenarios.is_empty() {
                format!("{}/{} passed", c.passed, c.total)
            } else {
                format!(
                    "{}/{} passed   FAILED: {}",
                    c.passed,
                    c.total,
                    c.failed_scenarios.join(", ")
                )
            };
            let _ = writeln!(out, " Correctness      {}", label);
        }
        None => {
            let _ = writeln!(out, " Correctness      skipped");
        }
    }

    match &summary.load {
        Some(l) => {
            let budget_note = if l.passed_budget {
                format!("p95 {}ms / budget {}ms", l.p95_ms, l.p95_budget_ms)
            } else {
                format!("p95 {}ms EXCEEDS budget {}ms", l.p95_ms, l.p95_budget_ms)
            };
            let _ = writeln!(
                out,
                " Load             {}/{} passed   {}",
                l.passed, l.total, budget_note,
            );
        }
        None => {
            let _ = writeln!(out, " Load             skipped");
        }
    }

    match &summary.passthrough {
        PassthroughSummary::NotRun => {
            let _ = writeln!(out, " Pass-through     skipped (no --contest-id)");
        }
        PassthroughSummary::Skipped { reason } => {
            let _ = writeln!(out, " Pass-through     skipped ({reason})");
        }
        PassthroughSummary::Completed { ok, count } => {
            let label = if *ok { "passed" } else { "FAILED" };
            let _ = writeln!(out, " Pass-through     {} {}", count, label);
        }
    }

    if let Some(l) = &summary.load {
        let acc = if l.completed == 0 {
            0.0
        } else {
            (l.passed as f64) / (l.completed as f64) * 100.0
        };
        let _ = writeln!(out, " Verdict accuracy {:.1}%", acc);
        let _ = writeln!(out, " Errors           {}", l.error_count);
    }

    let _ = writeln!(out);

    if summary.passed() {
        let _ = writeln!(out, " System is ready for contest.");
    } else {
        let _ = writeln!(out, " Issues:");
        if let Some(err) = &summary.bootstrap_error {
            let _ = writeln!(out, "   • bootstrap failed: {err}");
        }
        if let Some(c) = &summary.correctness {
            for id in &c.failed_scenarios {
                let _ = writeln!(
                    out,
                    "   • correctness scenario {id} did not match expectation"
                );
            }
        }
        if let Some(l) = &summary.load {
            if l.completed != l.total {
                let _ = writeln!(
                    out,
                    "   • only {}/{} load submissions reached terminal status",
                    l.completed, l.total
                );
            }
            if l.passed != l.completed {
                let _ = writeln!(
                    out,
                    "   • {} of {} completed load submissions had wrong verdict",
                    l.completed - l.passed,
                    l.completed
                );
            }
            if !l.passed_budget {
                let _ = writeln!(
                    out,
                    "   • p95 latency {}ms exceeds budget {}ms",
                    l.p95_ms, l.p95_budget_ms
                );
            }
            if l.error_count > 0 {
                let _ = writeln!(out, "   • {} HTTP / network errors", l.error_count);
            }
        }
        if let PassthroughSummary::Completed { ok: false, count } = summary.passthrough {
            let _ = writeln!(
                out,
                "   • pass-through phase failed across {} submissions",
                count,
            );
        }
        let _ = writeln!(out);
        let _ = writeln!(out, " DO NOT RUN CONTEST until these are resolved.");
    }

    if !summary.cleanup_warnings.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, " Cleanup warnings:");
        for w in &summary.cleanup_warnings {
            let _ = writeln!(out, "   • {w}");
        }
    }

    let _ = writeln!(
        out,
        "─────────────────────────────────────────────────────────"
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass_summary() -> RunSummary {
        RunSummary {
            target_url: "http://localhost:3000".into(),
            duration: Duration::from_secs_f64(21.7),
            bootstrap_error: None,
            correctness: Some(CorrectnessSummary {
                total: 9,
                passed: 9,
                failed_scenarios: vec![],
            }),
            load: Some(LoadSummary {
                total: 200,
                completed: 200,
                passed: 200,
                p50_ms: 820,
                p95_ms: 2104,
                p99_ms: 3401,
                max_ms: 4012,
                p95_budget_ms: 15000,
                passed_budget: true,
                error_count: 0,
                passed_overall: true,
            }),
            passthrough: PassthroughSummary::NotRun,
            cleanup_warnings: vec![],
        }
    }

    #[test]
    fn pass_summary_renders_pass_banner_and_ready_message() {
        let s = pass_summary();
        let out = format_summary(&s);
        assert!(out.contains("RESULT: PASS"));
        assert!(out.contains("9/9 passed"));
        assert!(out.contains("200/200 passed"));
        assert!(out.contains("p95 2104ms / budget 15000ms"));
        assert!(out.contains("System is ready for contest."));
        assert!(!out.contains("DO NOT RUN CONTEST"));
    }

    #[test]
    fn correctness_failure_renders_fail_banner_and_named_scenario() {
        let mut s = pass_summary();
        s.correctness = Some(CorrectnessSummary {
            total: 9,
            passed: 8,
            failed_scenarios: vec!["ab-cpp-mle".into()],
        });
        let out = format_summary(&s);
        assert!(out.contains("RESULT: FAIL"));
        assert!(out.contains("FAILED: ab-cpp-mle"));
        assert!(out.contains("ab-cpp-mle did not match expectation"));
        assert!(out.contains("DO NOT RUN CONTEST"));
    }

    #[test]
    fn load_budget_violation_renders_exceeds_message() {
        let mut s = pass_summary();
        let load = s.load.as_mut().unwrap();
        load.passed_budget = false;
        load.p95_ms = 18204;
        load.passed_overall = false;
        let out = format_summary(&s);
        assert!(out.contains("EXCEEDS budget"));
        assert!(out.contains("p95 latency 18204ms exceeds budget 15000ms"));
    }

    #[test]
    fn skipped_phases_render_explicit_skipped() {
        let s = RunSummary {
            target_url: "http://x".into(),
            duration: Duration::from_secs(1),
            bootstrap_error: None,
            correctness: None,
            load: None,
            passthrough: PassthroughSummary::Skipped {
                reason: "no --contest-id".into(),
            },
            cleanup_warnings: vec![],
        };
        let out = format_summary(&s);
        assert!(out.contains("Correctness      skipped"));
        assert!(out.contains("Load             skipped"));
        assert!(out.contains("Pass-through     skipped (no --contest-id)"));
        assert!(out.contains("RESULT: PASS"));
    }

    #[test]
    fn cleanup_warnings_attach_below_main_block() {
        let mut s = pass_summary();
        s.cleanup_warnings = vec!["could not delete problem 17: 404".into()];
        let out = format_summary(&s);
        assert!(out.contains("Cleanup warnings:"));
        assert!(out.contains("could not delete problem 17"));
    }

    #[test]
    fn bootstrap_error_renders_fail_and_names_the_error() {
        let mut s = pass_summary();
        s.bootstrap_error = Some("network error: connection refused".into());
        s.correctness = None;
        s.load = None;
        let out = format_summary(&s);
        assert!(out.contains("RESULT: FAIL"));
        assert!(out.contains("bootstrap failed: network error"));
        assert!(out.contains("DO NOT RUN CONTEST"));
    }
}
