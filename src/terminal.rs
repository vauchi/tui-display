// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! The crossterm terminal: native input with bounded polling, rendered by the session.

use crate::{SessionView, TerminalSession, ui::presentation_input::KeyOutcome};
use crossterm::event::{self, Event as NativeEvent, KeyCode, KeyEventKind, KeyModifiers};
use ratatui_core::{backend::Backend, terminal::Terminal};
use std::{
    io,
    ops::ControlFlow,
    time::{Duration, Instant},
};
use vauchi_contract::{Command, Event};

use crate::host::TerminalIo;

pub struct TerminalMode {
    active: bool,
}

impl TerminalMode {
    pub fn enter() -> io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        let mode = Self { active: true };
        crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
        Ok(mode)
    }

    fn restore(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let screen = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let raw = crossterm::terminal::disable_raw_mode();
        screen.and(raw)
    }
}

impl Drop for TerminalMode {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

pub trait TerminalEvents {
    fn next(&mut self, timeout: Duration) -> io::Result<Option<NativeEvent>>;

    /// The time a status is aged against.
    fn now(&self) -> Instant {
        Instant::now()
    }
}

pub struct CrosstermEvents;
impl TerminalEvents for CrosstermEvents {
    fn next(&mut self, timeout: Duration) -> io::Result<Option<NativeEvent>> {
        if event::poll(timeout)? {
            event::read().map(Some)
        } else {
            Ok(None)
        }
    }
}

pub use crate::host::TerminalFeedback;

/// How long a status stays up: it covers the screen's top rows, and the
/// `tui` binary this host replaces took it away after the same time.
const STATUS_TIME: Duration = Duration::from_secs(3);

pub struct TuiTerminal<'a, B: Backend, I, F> {
    output: &'a mut Terminal<B>,
    input: I,
    effects: F,
    session: TerminalSession,
    feedback: TerminalFeedback,
    status_since: Option<Instant>,
    mode: Option<TerminalMode>,
}

impl<'a, B: Backend, I: TerminalEvents, F> TuiTerminal<'a, B, I, F>
where
    F: FnMut(&[Command], &mut TerminalFeedback) -> io::Result<()>,
{
    pub fn new(output: &'a mut Terminal<B>, input: I, effects: F) -> Self {
        Self {
            output,
            input,
            effects,
            session: TerminalSession::default(),
            feedback: TerminalFeedback::default(),
            status_since: None,
            mode: None,
        }
    }

    pub fn with_mode(mut self, mode: TerminalMode) -> Self {
        self.mode = Some(mode);
        self
    }
}

impl<B: Backend, I, F> TuiTerminal<'_, B, I, F>
where
    B::Error: std::error::Error + Send + Sync + 'static,
{
    fn draw(&mut self) -> io::Result<()> {
        self.output
            .draw(|frame| {
                self.session.draw(
                    frame,
                    frame.area(),
                    SessionView {
                        status: self.feedback.status.as_deref(),
                        alert: self.feedback.alert.as_ref(),
                        ..SessionView::default()
                    },
                );
            })
            .map_err(io::Error::other)?;
        Ok(())
    }
}

impl<B: Backend, I: TerminalEvents, F> TerminalIo for TuiTerminal<'_, B, I, F>
where
    B::Error: std::error::Error + Send + Sync + 'static,
    F: FnMut(&[Command], &mut TerminalFeedback) -> io::Result<()>,
{
    fn restore(&mut self) -> io::Result<()> {
        let cursor = self.output.show_cursor().map_err(io::Error::other);
        let mode = self.mode.take().map_or(Ok(()), |mut mode| mode.restore());
        cursor.and(mode)
    }

    fn render(&mut self, commands: &[Command]) -> io::Result<ControlFlow<()>> {
        let effects = self.session.apply_commands(commands);
        let shown = self.feedback.status.take();
        (self.effects)(&effects, &mut self.feedback)?;
        if self.feedback.status.is_some() {
            self.status_since = Some(self.input.now());
        } else {
            self.feedback.status = shown;
        }
        if self.feedback.quit || self.session.presentation().native_back_requested() {
            return Ok(ControlFlow::Break(()));
        }
        self.draw()?;
        Ok(ControlFlow::Continue(()))
    }

    fn wait_input(&mut self, timeout: Duration) -> io::Result<ControlFlow<(), Vec<Event>>> {
        if self
            .status_since
            .is_some_and(|since| self.input.now().saturating_duration_since(since) >= STATUS_TIME)
        {
            self.status_since = None;
            self.feedback.status = None;
            self.draw()?;
        }
        let window = self.output.size().map_err(io::Error::other)?;
        let mut reports = self.session.take_reports();
        reports.extend(self.session.window_report(window.width, window.height));
        if !reports.is_empty() {
            return Ok(ControlFlow::Continue(reports));
        }
        let Some(event) = self.input.next(timeout)? else {
            return Ok(ControlFlow::Continue(vec![]));
        };
        match event {
            NativeEvent::Key(key) if key.kind != KeyEventKind::Release => {
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(ControlFlow::Break(()));
                }
                if self.feedback.alert.is_some() {
                    if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                        self.feedback.alert = None;
                        self.draw()?;
                    }
                    return Ok(ControlFlow::Continue(vec![]));
                }
                match self.session.handle_key(key) {
                    KeyOutcome::Events(events) => return Ok(ControlFlow::Continue(events)),
                    KeyOutcome::Quit => return Ok(ControlFlow::Break(())),
                    KeyOutcome::Consumed => self.draw()?,
                }
            }
            NativeEvent::Resize(..) => self.draw()?,
            _ => {}
        }
        Ok(ControlFlow::Continue(vec![]))
    }
}
