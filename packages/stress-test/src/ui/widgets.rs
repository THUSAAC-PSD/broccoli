use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Row, Sparkline, Table};

use crate::ui::app::{AppState, LogSeverity, PhaseState};
use crate::ui::theme::{ColorToken, PhaseGlyph, Theme};

pub fn themed_inner_block<'a>(theme: &Theme, title: &'a str) -> Block<'a> {
    Block::default()
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme.color(ColorToken::Accent))
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_set(theme.inner_border_set())
        .border_style(Style::default().fg(theme.color(ColorToken::Dim)))
}

pub fn themed_outer_block<'a>(theme: &Theme, title: &'a str) -> Block<'a> {
    Block::default()
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme.color(ColorToken::Accent))
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_set(theme.outer_border_set())
        .border_style(Style::default().fg(theme.color(ColorToken::Dim)))
}

pub fn render_phase_ladder(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let block = themed_inner_block(theme, "PHASES");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        phase_line(
            theme,
            state.correctness_state,
            "Correctness",
            state.correctness_progress,
        ),
        phase_line(theme, state.load_state, "Load", state.load_progress),
        phase_line(
            theme,
            state.passthrough_state,
            "Pass through",
            state.passthrough_progress,
        ),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn phase_line(
    theme: &Theme,
    state: PhaseState,
    label: &str,
    progress: (usize, usize),
) -> Line<'static> {
    let glyph_kind = match state {
        PhaseState::Pending => PhaseGlyph::Pending,
        PhaseState::Running => PhaseGlyph::Running,
        PhaseState::Passed => PhaseGlyph::Passed,
        PhaseState::Failed => PhaseGlyph::Failed,
        PhaseState::Skipped => PhaseGlyph::Skipped,
    };
    let token = match state {
        PhaseState::Pending | PhaseState::Skipped => ColorToken::Dim,
        PhaseState::Running => ColorToken::Accent,
        PhaseState::Passed => ColorToken::Ok,
        PhaseState::Failed => ColorToken::Err,
    };
    let counter = match state {
        PhaseState::Pending | PhaseState::Skipped => "  -  ".to_string(),
        _ => format!("{}/{}", progress.0, progress.1),
    };
    Line::from(vec![
        Span::raw(" "),
        Span::styled(
            theme.phase_glyph(glyph_kind).to_string(),
            Style::default().fg(theme.color(token)),
        ),
        Span::raw(" "),
        Span::raw(format!("{:<14}", label)),
        Span::styled(counter, Style::default().fg(theme.color(token))),
    ])
}

pub fn render_throughput_sparkline(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let block = themed_inner_block(theme, "THROUGHPUT subs/sec, last 60s");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 {
        return;
    }

    let data: Vec<u64> = state.throughput_buckets.iter().copied().collect();
    let sparkline_area = Rect::new(inner.x, inner.y, inner.width, inner.height - 1);
    let footer_area = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);

    let sparkline = Sparkline::default()
        .data(&data)
        .style(Style::default().fg(theme.color(ColorToken::Warn)));
    frame.render_widget(sparkline, sparkline_area);

    let footer = format!(
        " peak {} / sustained {:.1}",
        state.throughput_peak(),
        state.throughput_sustained()
    );
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(theme.color(ColorToken::Dim))),
        footer_area,
    );
}

pub fn render_latency_bars(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let block = themed_inner_block(theme, "LATENCY ms");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let display_max = state
        .p95_budget_ms
        .max(state.latency_max_ms.max(state.latency_p99_ms))
        .max(1);

    let bar_width = inner.width.saturating_sub(20) as usize;

    let rows: Vec<Line> = [
        ("p50", state.latency_p50_ms, ColorToken::Ok),
        ("p95", state.latency_p95_ms, ColorToken::Warn),
        ("p99", state.latency_p99_ms, ColorToken::Warn),
        ("max", state.latency_max_ms, ColorToken::Err),
    ]
    .into_iter()
    .map(|(label, value, color)| {
        let filled = if display_max == 0 {
            0
        } else {
            ((value as u128 * bar_width as u128) / display_max as u128) as usize
        };
        let filled = filled.min(bar_width);
        let bar_filled: String = "\u{2588}".repeat(filled);
        let bar_empty: String = "\u{2591}".repeat(bar_width.saturating_sub(filled));
        Line::from(vec![
            Span::raw(format!(" {:<4}", label)),
            Span::styled(bar_filled, Style::default().fg(theme.color(color))),
            Span::styled(bar_empty, Style::default().fg(theme.color(ColorToken::Dim))),
            Span::raw(format!(" {:>6}", value)),
        ])
    })
    .collect();

    let mut all = rows;
    all.push(Line::from(Span::styled(
        format!(" budget p95 < {}", state.p95_budget_ms),
        Style::default().fg(theme.color(ColorToken::Dim)),
    )));

    frame.render_widget(Paragraph::new(all), inner);
}

pub fn render_verdict_chart(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let block = themed_inner_block(theme, "VERDICTS");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let entries = state.verdicts_sorted();
    let max = entries.iter().map(|(_, c)| *c).max().unwrap_or(0).max(1);
    let bar_width = inner.width.saturating_sub(24) as usize;

    let lines: Vec<Line> = entries
        .into_iter()
        .take(inner.height as usize)
        .map(|(name, count)| {
            let filled = ((count as u128 * bar_width as u128) / max as u128) as usize;
            let filled = filled.min(bar_width);
            let token = verdict_color(&name);
            let bar: String = "\u{2588}".repeat(filled);
            Line::from(vec![
                Span::raw(format!(" {:<18}", truncate(&name, 18))),
                Span::styled(bar, Style::default().fg(theme.color(token))),
                Span::raw(format!(" {:>4}", count)),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max - 1).collect::<String>() + "\u{2026}"
    }
}

fn verdict_color(name: &str) -> ColorToken {
    match name {
        "Accepted" => ColorToken::Ok,
        _ => ColorToken::Err,
    }
}

pub fn render_event_log(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let title = if state.log_paused {
        "EVENT LOG paused"
    } else {
        "EVENT LOG"
    };
    let block = themed_inner_block(theme, title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let visible = inner.height as usize;
    let total = state.event_log.len();
    let scroll = state.log_scroll_offset.min(total.saturating_sub(visible));
    let start = total.saturating_sub(visible + scroll);
    let end = total.saturating_sub(scroll);

    let rows: Vec<Row> = state
        .event_log
        .iter()
        .skip(start)
        .take(end - start)
        .map(|entry| {
            let (label, token) = match entry.severity {
                LogSeverity::Ok => ("OK ", ColorToken::Ok),
                LogSeverity::Warn => ("WRN", ColorToken::Warn),
                LogSeverity::Err => ("ERR", ColorToken::Err),
            };
            let ts = entry.timestamp.format("%H:%M:%S").to_string();
            Row::new(vec![
                ts,
                label.to_string(),
                entry.phase.clone(),
                entry.message.clone(),
            ])
            .style(Style::default().fg(theme.color(token)))
        })
        .collect();

    let widths = [
        Constraint::Length(8),
        Constraint::Length(3),
        Constraint::Length(13),
        Constraint::Min(0),
    ];
    let table = Table::new(rows, widths);
    frame.render_widget(table, inner);
}

pub fn render_in_flight(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let title = format!("IN FLIGHT {}/{}", state.in_flight, state.concurrency);
    let block = themed_inner_block(theme, &title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let token = if state.in_flight_ratio() > 0.85 {
        ColorToken::Warn
    } else {
        ColorToken::Ok
    };
    let gauge = Gauge::default()
        .ratio(state.in_flight_ratio())
        .gauge_style(Style::default().fg(theme.color(token)))
        .label(format!("{} / {}", state.in_flight, state.concurrency));
    frame.render_widget(gauge, inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app::LogEntry;
    use crate::ui::theme::{Capability, GlyphSet};
    use chrono::TimeZone;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    fn ascii_theme() -> Theme {
        Theme::new(Capability::Ansi16, GlyphSet::Ascii)
    }

    fn unicode_theme() -> Theme {
        Theme::new(Capability::Truecolor, GlyphSet::Unicode)
    }

    fn buffer_lines(buf: &Buffer) -> Vec<String> {
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
                    .collect::<String>()
            })
            .collect()
    }

    fn render_to_lines<F>(width: u16, height: u16, draw: F) -> Vec<String>
    where
        F: Fn(&mut Frame, Rect),
    {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|f| {
                let area = f.area();
                draw(f, area);
            })
            .expect("draw");
        buffer_lines(terminal.backend().buffer())
    }

    fn make_state() -> AppState {
        AppState::new("http://x".into(), 15000, 50)
    }

    #[test]
    fn phase_ladder_shows_three_phases_with_counters_ascii() {
        let theme = ascii_theme();
        let mut state = make_state();
        state.correctness_state = PhaseState::Passed;
        state.correctness_progress = (9, 9);
        state.load_state = PhaseState::Running;
        state.load_progress = (142, 200);
        state.passthrough_state = PhaseState::Pending;

        let lines = render_to_lines(34, 5, |f, a| render_phase_ladder(f, a, &state, &theme));
        let body = lines.join("\n");
        assert!(body.contains("[x] Correctness"));
        assert!(body.contains("9/9"));
        assert!(body.contains("[*] Load"));
        assert!(body.contains("142/200"));
        assert!(body.contains("[ ] Pass through"));
    }

    #[test]
    fn phase_ladder_skipped_passthrough_uses_dash_glyph_ascii() {
        let theme = ascii_theme();
        let mut state = make_state();
        state.passthrough_state = PhaseState::Skipped;
        let lines = render_to_lines(34, 5, |f, a| render_phase_ladder(f, a, &state, &theme));
        assert!(lines.join("\n").contains("[-] Pass through"));
    }

    #[test]
    fn phase_ladder_failed_load_uses_failed_glyph_ascii() {
        let theme = ascii_theme();
        let mut state = make_state();
        state.load_state = PhaseState::Failed;
        state.load_progress = (180, 200);
        let lines = render_to_lines(34, 5, |f, a| render_phase_ladder(f, a, &state, &theme));
        let body = lines.join("\n");
        assert!(body.contains("[!] Load"));
        assert!(body.contains("180/200"));
    }

    #[test]
    fn throughput_widget_shows_peak_and_sustained_ascii() {
        let theme = ascii_theme();
        let mut state = make_state();
        state.throughput_buckets.extend([10, 20, 30, 20]);
        let lines = render_to_lines(40, 5, |f, a| {
            render_throughput_sparkline(f, a, &state, &theme)
        });
        let body = lines.join("\n");
        assert!(body.contains("THROUGHPUT"));
        assert!(body.contains("peak 30"));
        assert!(body.contains("sustained 20.0"));
    }

    #[test]
    fn latency_bars_renders_all_four_with_values() {
        let theme = ascii_theme();
        let mut state = make_state();
        state.latency_p50_ms = 820;
        state.latency_p95_ms = 2104;
        state.latency_p99_ms = 3401;
        state.latency_max_ms = 4012;
        let lines = render_to_lines(30, 7, |f, a| render_latency_bars(f, a, &state, &theme));
        let body = lines.join("\n");
        assert!(body.contains("p50"));
        assert!(body.contains("820"));
        assert!(body.contains("p95"));
        assert!(body.contains("2104"));
        assert!(body.contains("p99"));
        assert!(body.contains("3401"));
        assert!(body.contains("max"));
        assert!(body.contains("4012"));
        assert!(body.contains("budget p95 < 15000"));
    }

    #[test]
    fn verdict_chart_lists_counts_in_descending_order() {
        let theme = ascii_theme();
        let mut state = make_state();
        state.verdict_counts.insert("Accepted".into(), 98);
        state.verdict_counts.insert("WrongAnswer".into(), 12);
        state.verdict_counts.insert("RuntimeError".into(), 5);
        let lines = render_to_lines(40, 8, |f, a| render_verdict_chart(f, a, &state, &theme));
        let acc_idx = lines.iter().position(|l| l.contains("Accepted")).unwrap();
        let wa_idx = lines
            .iter()
            .position(|l| l.contains("WrongAnswer"))
            .unwrap();
        let re_idx = lines
            .iter()
            .position(|l| l.contains("RuntimeError"))
            .unwrap();
        assert!(acc_idx < wa_idx);
        assert!(wa_idx < re_idx);
        assert!(lines.iter().any(|l| l.contains("98")));
    }

    #[test]
    fn event_log_renders_recent_entries() {
        let theme = ascii_theme();
        let mut state = make_state();
        let ts = chrono::Utc
            .with_ymd_and_hms(2026, 5, 1, 14, 32, 18)
            .unwrap();
        state.push_log(LogEntry {
            timestamp: ts,
            severity: LogSeverity::Ok,
            phase: "load".into(),
            message: "submission #142 Accepted".into(),
        });
        state.push_log(LogEntry {
            timestamp: ts,
            severity: LogSeverity::Err,
            phase: "load".into(),
            message: "submission #143 WrongAnswer".into(),
        });
        let lines = render_to_lines(60, 6, |f, a| render_event_log(f, a, &state, &theme));
        let body = lines.join("\n");
        assert!(body.contains("14:32:18"));
        assert!(body.contains("OK"));
        assert!(body.contains("ERR"));
        assert!(body.contains("Accepted"));
        assert!(body.contains("WrongAnswer"));
    }

    #[test]
    fn event_log_paused_title_changes() {
        let theme = ascii_theme();
        let mut state = make_state();
        state.log_paused = true;
        let lines = render_to_lines(40, 4, |f, a| render_event_log(f, a, &state, &theme));
        assert!(lines.join("\n").contains("paused"));
    }

    #[test]
    fn in_flight_gauge_shows_label_with_counter() {
        let theme = ascii_theme();
        let mut state = make_state();
        state.in_flight = 25;
        state.concurrency = 50;
        let lines = render_to_lines(30, 3, |f, a| render_in_flight(f, a, &state, &theme));
        let body = lines.join("\n");
        assert!(body.contains("IN FLIGHT 25/50"));
        assert!(body.contains("25 / 50"));
    }

    #[test]
    fn in_flight_gauge_zero_concurrency_does_not_panic() {
        let theme = ascii_theme();
        let mut state = make_state();
        state.concurrency = 0;
        let lines = render_to_lines(30, 3, |f, a| render_in_flight(f, a, &state, &theme));
        assert!(!lines.is_empty());
    }

    #[test]
    fn unicode_theme_renders_box_drawing_borders() {
        let theme = unicode_theme();
        let state = make_state();
        let lines = render_to_lines(34, 5, |f, a| render_phase_ladder(f, a, &state, &theme));
        let body = lines.join("\n");
        assert!(body.contains('\u{250C}') || body.contains('\u{2500}'));
    }
}
