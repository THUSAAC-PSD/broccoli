use std::io::{self, Write};
use std::time::Duration;

use crossterm::event::{self, Event as CtEvent, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval};

use crate::events::Event;
use crate::ui::app::AppState;
use crate::ui::theme::Theme;
use crate::ui::widgets::render_dashboard;

const TICK_RATE: Duration = Duration::from_millis(250);
const KEY_POLL: Duration = Duration::from_millis(20);

pub async fn run_tui(
    rx: mpsc::UnboundedReceiver<Event>,
    target_url: String,
    p95_budget_ms: u64,
    concurrency: usize,
) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let theme = Theme::from_env();
    let state = AppState::new(target_url, p95_budget_ms, concurrency);

    let result = drive_loop(&mut terminal, rx, state, &theme).await;

    let _ = disable_raw_mode();
    let _ = crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();
    let _ = io::stdout().flush();

    result
}

async fn drive_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut rx: mpsc::UnboundedReceiver<Event>,
    mut state: AppState,
    theme: &Theme,
) -> io::Result<()> {
    let (key_tx, mut key_rx) = mpsc::unbounded_channel::<KeyEvent>();
    spawn_key_reader(key_tx);

    let mut tick = interval(TICK_RATE);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut closed = false;
    let mut quit = false;

    loop {
        tokio::select! {
            biased;
            recv = rx.recv(), if !closed => {
                match recv {
                    Some(event) => state.apply_event(&event),
                    None => closed = true,
                }
            }
            Some(key) = key_rx.recv() => {
                if handle_key(&mut state, key) {
                    quit = true;
                }
            }
            _ = tick.tick() => {
                state.tick(std::time::Instant::now());
                terminal.draw(|f| render_dashboard(f, f.area(), &state, theme))?;
            }
        }

        if quit || (closed && rx.is_empty()) {
            terminal.draw(|f| render_dashboard(f, f.area(), &state, theme))?;
            break;
        }
    }
    Ok(())
}

fn handle_key(state: &mut AppState, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => true,
        KeyCode::Char('p') => {
            state.log_paused = !state.log_paused;
            false
        }
        KeyCode::Up => {
            state.log_scroll_offset = state.log_scroll_offset.saturating_add(1);
            false
        }
        KeyCode::Down => {
            state.log_scroll_offset = state.log_scroll_offset.saturating_sub(1);
            false
        }
        _ => false,
    }
}

fn spawn_key_reader(tx: mpsc::UnboundedSender<KeyEvent>) {
    tokio::task::spawn_blocking(move || {
        loop {
            if tx.is_closed() {
                break;
            }
            match event::poll(KEY_POLL) {
                Ok(true) => match event::read() {
                    Ok(CtEvent::Key(k)) => {
                        if tx.send(k).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                },
                Ok(false) => {}
                Err(_) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{SubmissionStatus, Verdict};
    use crate::events::{ActualTerminal, ExpectedTerminal, Phase};
    use crate::ui::theme::{Capability, GlyphSet};
    use ratatui::backend::TestBackend;

    fn ascii_theme() -> Theme {
        Theme::new(Capability::Ansi16, GlyphSet::Ascii)
    }

    #[tokio::test]
    async fn drive_loop_renders_canned_event_stream_until_channel_closes() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(Event::PhaseStarted {
            phase: Phase::Correctness,
            total: Some(9),
        })
        .unwrap();
        tx.send(Event::ScenarioStarted {
            id: "ab-cpp-ac".into(),
        })
        .unwrap();
        tx.send(Event::ScenarioFinished {
            id: "ab-cpp-ac".into(),
            ok: true,
            status: SubmissionStatus::Judged,
            verdict: Some(Verdict::Accepted),
            duration_ms: 412,
        })
        .unwrap();
        tx.send(Event::PhaseFinished {
            phase: Phase::Correctness,
            ok: true,
        })
        .unwrap();
        tx.send(Event::PhaseStarted {
            phase: Phase::Load,
            total: Some(200),
        })
        .unwrap();
        tx.send(Event::LoadSubmitted {
            sequence: 1,
            scenario: "ab-cpp-ac".into(),
        })
        .unwrap();
        tx.send(Event::LoadCompleted {
            sequence: 1,
            ok: true,
            latency_ms: 800,
            expected: ExpectedTerminal {
                status: SubmissionStatus::Judged,
                verdict: Some(Verdict::Accepted),
            },
            actual: ActualTerminal {
                status: SubmissionStatus::Judged,
                verdict: Some(Verdict::Accepted),
            },
        })
        .unwrap();
        drop(tx);

        let state = AppState::new("http://x".into(), 15_000, 50);
        let theme = ascii_theme();
        drive_loop(&mut terminal, rx, state, &theme)
            .await
            .expect("drive_loop ok");
    }

    #[test]
    fn handle_key_q_quits() {
        let mut s = AppState::new("http://x".into(), 15_000, 50);
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(handle_key(&mut s, key));
    }

    #[test]
    fn handle_key_esc_quits() {
        let mut s = AppState::new("http://x".into(), 15_000, 50);
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert!(handle_key(&mut s, key));
    }

    #[test]
    fn handle_key_ctrl_c_quits() {
        let mut s = AppState::new("http://x".into(), 15_000, 50);
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(handle_key(&mut s, key));
    }

    #[test]
    fn handle_key_p_toggles_pause() {
        let mut s = AppState::new("http://x".into(), 15_000, 50);
        let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE);
        assert!(!handle_key(&mut s, key));
        assert!(s.log_paused);
        assert!(!handle_key(&mut s, key));
        assert!(!s.log_paused);
    }

    #[test]
    fn handle_key_arrows_change_scroll_offset() {
        let mut s = AppState::new("http://x".into(), 15_000, 50);
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        handle_key(&mut s, up);
        handle_key(&mut s, up);
        handle_key(&mut s, up);
        assert_eq!(s.log_scroll_offset, 3);
        handle_key(&mut s, down);
        assert_eq!(s.log_scroll_offset, 2);
        for _ in 0..10 {
            handle_key(&mut s, down);
        }
        assert_eq!(s.log_scroll_offset, 0);
    }
}
