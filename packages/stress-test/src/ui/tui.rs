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
                terminal.draw(|f| render_dashboard(f, f.area(), &mut state, theme))?;
            }
        }

        if quit || (closed && rx.is_empty()) {
            terminal.draw(|f| render_dashboard(f, f.area(), &mut state, theme))?;
            break;
        }
    }
    Ok(())
}

fn handle_key(state: &mut AppState, key: KeyEvent) -> bool {
    let page = state.last_log_visible.saturating_sub(1).max(1);
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => true,
        KeyCode::Char('p') => {
            state.toggle_log_pause();
            false
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.scroll_log_up(1);
            false
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.scroll_log_down(1);
            false
        }
        KeyCode::PageUp => {
            state.scroll_log_up(page);
            false
        }
        KeyCode::PageDown => {
            state.scroll_log_down(page);
            false
        }
        KeyCode::Home | KeyCode::Char('g') => {
            state.scroll_log_oldest();
            false
        }
        KeyCode::End | KeyCode::Char('G') => {
            state.resume_log_tail();
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
        assert!(s.is_log_paused());
        assert!(!handle_key(&mut s, key));
        assert!(!s.is_log_paused());
    }

    fn fill_log(s: &mut AppState, n: usize) {
        use crate::ui::app::{LogEntry, LogSeverity};
        for i in 0..n {
            s.push_log(LogEntry {
                timestamp: chrono::Utc::now(),
                severity: LogSeverity::Ok,
                phase: "load".into(),
                message: format!("entry {i}"),
            });
        }
    }

    #[test]
    fn handle_key_up_auto_pauses_and_clamps_to_scrollable_range() {
        let mut s = AppState::new("http://x".into(), 15_000, 50);
        s.last_log_visible = 5;
        fill_log(&mut s, 8); // max scroll = 8 - 5 = 3

        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        handle_key(&mut s, up);
        assert!(s.is_log_paused(), "scrolling auto-pauses");
        assert_eq!(s.log_scroll_offset, 1);
        handle_key(&mut s, up);
        handle_key(&mut s, up);
        handle_key(&mut s, up);
        handle_key(&mut s, up); // try to overshoot
        assert_eq!(s.log_scroll_offset, 3, "clamped to max scroll");

        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        handle_key(&mut s, down);
        assert_eq!(s.log_scroll_offset, 2);
        for _ in 0..10 {
            handle_key(&mut s, down);
        }
        assert_eq!(s.log_scroll_offset, 0);
    }

    #[test]
    fn handle_key_pageup_pagedown_use_visible_height() {
        let mut s = AppState::new("http://x".into(), 15_000, 50);
        s.last_log_visible = 10;
        fill_log(&mut s, 100);

        let pgup = KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE);
        handle_key(&mut s, pgup);
        assert_eq!(s.log_scroll_offset, 9, "page = visible-1");

        let pgdn = KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE);
        handle_key(&mut s, pgdn);
        assert_eq!(s.log_scroll_offset, 0);
    }

    #[test]
    fn handle_key_home_jumps_to_oldest_end_resumes_tail() {
        let mut s = AppState::new("http://x".into(), 15_000, 50);
        s.last_log_visible = 5;
        fill_log(&mut s, 50); // max scroll = 45

        let home = KeyEvent::new(KeyCode::Home, KeyModifiers::NONE);
        handle_key(&mut s, home);
        assert!(s.is_log_paused());
        assert_eq!(s.log_scroll_offset, 45);

        let end = KeyEvent::new(KeyCode::End, KeyModifiers::NONE);
        handle_key(&mut s, end);
        assert!(!s.is_log_paused(), "End resumes follow-tail");
        assert_eq!(s.log_scroll_offset, 0);
    }

    #[test]
    fn paused_view_does_not_shift_when_new_events_arrive() {
        let mut s = AppState::new("http://x".into(), 15_000, 50);
        fill_log(&mut s, 20);
        s.toggle_log_pause();
        let snapshot_len = s.view_log().len();
        fill_log(&mut s, 30); // 30 more arrive while paused
        assert_eq!(
            s.view_log().len(),
            snapshot_len,
            "view stays frozen at pause-time snapshot"
        );
        assert_eq!(
            s.event_log.len(),
            50,
            "live log keeps growing in background"
        );
        s.resume_log_tail();
        assert_eq!(s.view_log().len(), 50, "resume reveals buffered events");
    }
}
