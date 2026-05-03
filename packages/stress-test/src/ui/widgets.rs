use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};

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

const DASH: &str = "—";

fn phase_glyph_token(state: PhaseState) -> (PhaseGlyph, ColorToken) {
    match state {
        PhaseState::Pending => (PhaseGlyph::Pending, ColorToken::Dim),
        PhaseState::Running => (PhaseGlyph::Running, ColorToken::Accent),
        PhaseState::Passed => (PhaseGlyph::Passed, ColorToken::Ok),
        PhaseState::Failed => (PhaseGlyph::Failed, ColorToken::Err),
        PhaseState::Skipped => (PhaseGlyph::Skipped, ColorToken::Dim),
    }
}

fn phase_segment(theme: &Theme, label: &str, state: PhaseState) -> Vec<Span<'static>> {
    let (glyph, token) = phase_glyph_token(state);
    let color = theme.color(token);
    vec![
        Span::styled(label.to_string(), Style::default().fg(color)),
        Span::raw(" "),
        Span::styled(
            theme.phase_glyph(glyph).to_string(),
            Style::default().fg(color),
        ),
    ]
}

fn render_phase_strip(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    spans.extend(phase_segment(theme, "Bootstrap", state.bootstrap_state));
    spans.push(Span::raw("   "));
    spans.extend(phase_segment(theme, "Correctness", state.correctness_state));
    if matches!(
        state.correctness_state,
        PhaseState::Running | PhaseState::Passed | PhaseState::Failed
    ) {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!(
                "{}/{}",
                state.correctness_progress.0, state.correctness_progress.1
            ),
            Style::default().fg(theme.color(ColorToken::Dim)),
        ));
    }
    spans.push(Span::raw("   "));
    spans.extend(phase_segment(theme, "Load", state.load_state));
    if matches!(
        state.load_state,
        PhaseState::Running | PhaseState::Passed | PhaseState::Failed
    ) {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("{}/{}", state.load_progress.0, state.load_progress.1),
            Style::default().fg(theme.color(ColorToken::Dim)),
        ));
    }
    spans.push(Span::raw("   "));
    spans.extend(phase_segment(
        theme,
        "Pass-through",
        state.passthrough_state,
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_latency_strip(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let dim = Style::default().fg(theme.color(ColorToken::Dim));
    let has_data = !state.latency_hist.is_empty();
    let value_style = |token: ColorToken| {
        if has_data {
            Style::default().fg(theme.color(token))
        } else {
            dim
        }
    };
    let fmt = |v: u64| {
        if has_data {
            v.to_string()
        } else {
            DASH.to_string()
        }
    };

    let p95_over = has_data && state.latency_p95_ms > state.p95_budget_ms;
    let p95_token = if p95_over {
        ColorToken::Err
    } else if has_data {
        ColorToken::Warn
    } else {
        ColorToken::Dim
    };

    let spans: Vec<Span<'static>> = vec![
        Span::styled(" Latency  ", dim),
        Span::styled("p50 ", dim),
        Span::styled(fmt(state.latency_p50_ms), value_style(ColorToken::Ok)),
        Span::styled("   p95 ", dim),
        Span::styled(
            fmt(state.latency_p95_ms),
            Style::default().fg(theme.color(p95_token)),
        ),
        Span::styled("   p99 ", dim),
        Span::styled(fmt(state.latency_p99_ms), value_style(ColorToken::Warn)),
        Span::styled("   max ", dim),
        Span::styled(fmt(state.latency_max_ms), value_style(ColorToken::Err)),
        Span::styled(format!("   budget p95 < {}ms", state.p95_budget_ms), dim),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn inflight_bar(state: &AppState) -> String {
    const WIDTH: usize = 8;
    let cap = state.concurrency.max(1);
    let filled = ((state.in_flight * WIDTH) / cap).min(WIDTH);
    let mut s = String::with_capacity(WIDTH * 3);
    for _ in 0..filled {
        s.push('\u{2588}');
    }
    for _ in 0..WIDTH.saturating_sub(filled) {
        s.push('\u{2591}');
    }
    s
}

fn render_load_strip(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let dim = Style::default().fg(theme.color(ColorToken::Dim));
    let has_throughput =
        !state.throughput_buckets.is_empty() && state.throughput_buckets.iter().any(|b| *b > 0);

    let inflight_token = if state.in_flight_ratio() > 0.85 {
        ColorToken::Warn
    } else {
        ColorToken::Ok
    };

    let peak = if has_throughput {
        state.throughput_peak().to_string()
    } else {
        DASH.to_string()
    };
    let sustained = if has_throughput {
        format!("{:.1}", state.throughput_sustained())
    } else {
        DASH.to_string()
    };

    let spans: Vec<Span<'static>> = vec![
        Span::styled(" In-flight ", dim),
        Span::styled(
            inflight_bar(state),
            Style::default().fg(theme.color(inflight_token)),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{}/{}", state.in_flight, state.concurrency),
            Style::default().fg(theme.color(inflight_token)),
        ),
        Span::styled("   peak ", dim),
        Span::styled(peak, Style::default().fg(theme.color(ColorToken::Accent))),
        Span::styled("/s   sustained ", dim),
        Span::styled(
            sustained,
            Style::default().fg(theme.color(ColorToken::Accent)),
        ),
        Span::styled("/s", dim),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn verdict_short(name: &str) -> &str {
    match name {
        "Accepted" => "AC",
        "WrongAnswer" => "WA",
        "TimeLimitExceeded" => "TLE",
        "MemoryLimitExceeded" => "MLE",
        "RuntimeError" => "RE",
        "CompilationError" => "CE",
        "SystemError" => "SE",
        "Pending" => "PEND",
        "Running" => "RUN",
        other => other,
    }
}

fn verdict_color(short: &str, theme: &Theme) -> Style {
    let token = if short == "AC" {
        ColorToken::Ok
    } else {
        ColorToken::Err
    };
    Style::default().fg(theme.color(token))
}

fn render_verdict_strip(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let dim = Style::default().fg(theme.color(ColorToken::Dim));
    let mut spans: Vec<Span<'static>> = vec![Span::styled(" Verdicts ", dim)];

    let entries = state.verdicts_sorted();
    if entries.is_empty() {
        spans.push(Span::styled(" ".to_string() + DASH, dim));
    } else {
        for (name, count) in entries {
            let short = verdict_short(&name).to_string();
            spans.push(Span::raw(" "));
            spans.push(Span::styled(short.clone(), verdict_color(&short, theme)));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(count.to_string(), dim));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
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

pub fn render_dashboard(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let header = format!(
        " BROCCOLI STRESS TEST  {}  {} ",
        state.target_url,
        state.elapsed_clock()
    );
    let outer = themed_outer_block(theme, &header);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let [r_phases, r_latency, r_load, r_verdicts, r_log, r_footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(inner);

    render_phase_strip(frame, r_phases, state, theme);
    render_latency_strip(frame, r_latency, state, theme);
    render_load_strip(frame, r_load, state, theme);
    render_verdict_strip(frame, r_verdicts, state, theme);
    render_event_log(frame, r_log, state, theme);

    let footer = Paragraph::new(" [q] quit  [p] pause  [up/down] scroll log")
        .style(Style::default().fg(theme.color(ColorToken::Dim)));
    frame.render_widget(footer, r_footer);
}
