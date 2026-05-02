use std::io::Write;

use chrono::Utc;
use tokio::sync::mpsc;

use crate::events::Event;

pub async fn run<W>(mut rx: mpsc::UnboundedReceiver<Event>, out: &mut W) -> std::io::Result<()>
where
    W: Write + Send + Unpin,
{
    while let Some(event) = rx.recv().await {
        let line = render_line(&event, Utc::now());
        writeln!(out, "{}", line)?;
        out.flush()?;
    }
    Ok(())
}

pub fn render_line(event: &Event, now: chrono::DateTime<Utc>) -> String {
    let ts = now.format("%H:%M:%SZ");

    fn fmt_optional_verdict(v: &Option<crate::dto::Verdict>) -> String {
        match v {
            Some(v) => format!("{:?}", v),
            None => "—".to_string(),
        }
    }

    match event {
        Event::PhaseStarted { phase } => format!("[{ts}] --- {:<12} starting", phase.label(),),
        Event::PhaseFinished { phase, ok } => {
            let status = if *ok { "OK " } else { "ERR" };
            format!("[{ts}] {status} {:<12} finished ok={}", phase.label(), ok)
        }
        Event::ScenarioStarted { id } => {
            format!("[{ts}] --- {:<12} scenario {} starting", "correctness", id,)
        }
        Event::ScenarioFinished {
            id,
            ok,
            status,
            verdict,
            duration_ms,
        } => {
            let prefix = if *ok { "OK " } else { "ERR" };
            format!(
                "[{ts}] {prefix} {:<12} scenario {} status={:?} verdict={} {}ms",
                "correctness",
                id,
                status,
                fmt_optional_verdict(verdict),
                duration_ms,
            )
        }
        Event::LoadSubmitted { sequence, scenario } => format!(
            "[{ts}] --- {:<12} #{} submitted ({})",
            "load", sequence, scenario,
        ),
        Event::LoadCompleted {
            sequence,
            ok,
            latency_ms,
            expected,
            actual,
        } => {
            let prefix = if *ok { "OK " } else { "ERR" };
            format!(
                "[{ts}] {prefix} {:<12} #{} expected=({:?},{}) actual=({:?},{}) {}ms",
                "load",
                sequence,
                expected.status,
                fmt_optional_verdict(&expected.verdict),
                actual.status,
                fmt_optional_verdict(&actual.verdict),
                latency_ms,
            )
        }
        Event::PassthroughSkipped { reason } => {
            format!("[{ts}] WRN {:<12} skipped: {}", "passthrough", reason,)
        }
        Event::PassthroughCompleted { ok, count } => {
            let prefix = if *ok { "OK " } else { "ERR" };
            format!(
                "[{ts}] {prefix} {:<12} {} submissions",
                "passthrough", count,
            )
        }
        Event::Error { phase, message } => {
            let p = phase.map(|p| p.label()).unwrap_or("(global)");
            format!("[{ts}] ERR {:<12} {}", p, message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{SubmissionStatus, Verdict};
    use crate::events::{ActualTerminal, ExpectedTerminal, Phase};
    use chrono::TimeZone;

    fn fixed_now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 1, 14, 32, 18).unwrap()
    }

    #[test]
    fn renders_phase_started() {
        let line = render_line(
            &Event::PhaseStarted {
                phase: Phase::Correctness,
            },
            fixed_now(),
        );
        assert_eq!(line, "[14:32:18Z] --- correctness  starting");
    }

    #[test]
    fn renders_scenario_finished_pass() {
        let line = render_line(
            &Event::ScenarioFinished {
                id: "ab-cpp-ac".into(),
                ok: true,
                status: SubmissionStatus::Judged,
                verdict: Some(Verdict::Accepted),
                duration_ms: 412,
            },
            fixed_now(),
        );
        assert!(line.starts_with("[14:32:18Z] OK  correctness"));
        assert!(line.contains("ab-cpp-ac"));
        assert!(line.contains("Accepted"));
        assert!(line.contains("412ms"));
    }

    #[test]
    fn renders_scenario_finished_fail() {
        let line = render_line(
            &Event::ScenarioFinished {
                id: "ab-cpp-mle".into(),
                ok: false,
                status: SubmissionStatus::Judged,
                verdict: Some(Verdict::WrongAnswer),
                duration_ms: 980,
            },
            fixed_now(),
        );
        assert!(line.starts_with("[14:32:18Z] ERR correctness"));
        assert!(line.contains("WrongAnswer"));
    }

    #[test]
    fn renders_load_completed_with_expected_actual() {
        let line = render_line(
            &Event::LoadCompleted {
                sequence: 142,
                ok: false,
                latency_ms: 820,
                expected: ExpectedTerminal {
                    status: SubmissionStatus::Judged,
                    verdict: Some(Verdict::Accepted),
                },
                actual: ActualTerminal {
                    status: SubmissionStatus::Judged,
                    verdict: Some(Verdict::WrongAnswer),
                },
            },
            fixed_now(),
        );
        assert!(line.contains("ERR"));
        assert!(line.contains("#142"));
        assert!(line.contains("Accepted"));
        assert!(line.contains("WrongAnswer"));
        assert!(line.contains("820ms"));
    }

    #[test]
    fn renders_passthrough_skipped() {
        let line = render_line(
            &Event::PassthroughSkipped {
                reason: "no --contest-id".into(),
            },
            fixed_now(),
        );
        assert!(line.starts_with("[14:32:18Z] WRN passthrough"));
        assert!(line.contains("no --contest-id"));
    }

    #[test]
    fn renders_error_without_phase() {
        let line = render_line(
            &Event::Error {
                phase: None,
                message: "boom".into(),
            },
            fixed_now(),
        );
        assert!(line.contains("(global)"));
        assert!(line.contains("boom"));
    }

    #[tokio::test]
    async fn run_drains_channel_to_writer_until_closed() {
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(Event::PhaseStarted {
            phase: Phase::Correctness,
        })
        .unwrap();
        tx.send(Event::PhaseFinished {
            phase: Phase::Correctness,
            ok: true,
        })
        .unwrap();
        drop(tx);

        let mut buf: Vec<u8> = Vec::new();
        run(rx, &mut buf).await.expect("run ok");

        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("correctness"));
        assert!(lines[0].contains("starting"));
        assert!(lines[1].contains("OK"));
        assert!(lines[1].contains("ok=true"));
    }
}
