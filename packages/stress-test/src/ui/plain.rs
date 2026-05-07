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
        Event::PhaseStarted { phase, .. } => format!("[{ts}] --- {:<12} starting", phase.label(),),
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
    use crate::events::Phase;

    #[tokio::test]
    async fn run_drains_channel_to_writer_until_closed() {
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(Event::PhaseStarted {
            phase: Phase::Correctness,
            total: None,
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
