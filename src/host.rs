// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! The terminal host loop, shared by every app that assembles this display.
//!
//! The app builds Core and its hardware shell and hands both over as one
//! [`TerminalHost`]; the loop names no OS (private#1234).

use std::{io, ops::ControlFlow, time::Duration};
use vauchi_contract::{Command, Event};

/// How long the loop waits for input before it polls the host again, so
/// background repaints and hardware observations progress without a key.
const INPUT_WAIT: Duration = Duration::from_millis(50);

/// The app's side of the loop: Core and the hardware shell behind its routing.
pub trait TerminalHost {
    type Error: Into<Box<dyn std::error::Error + Send + Sync>>;

    /// Hand one event to Core and route the commands it returns.
    fn dispatch(&mut self, event: Event) -> Result<(), Self::Error>;

    /// Process a pending repaint and one ready hardware observation;
    /// true when either produced work.
    fn poll(&mut self) -> Result<bool, Self::Error>;

    /// The display commands routed since the last call.
    fn take_commands(&mut self) -> Vec<Command>;
}

#[derive(Default)]
pub struct TerminalFeedback {
    pub status: Option<String>,
    pub alert: Option<(String, String)>,
    /// Core wiped its data: the app ends so the next start is a fresh one.
    pub quit: bool,
}

pub trait TerminalIo {
    /// Apply a batch and render it; break when the display requests exit.
    fn render(&mut self, commands: &[Command]) -> io::Result<ControlFlow<()>>;

    /// Return input or exit, waiting no longer than `timeout` when idle.
    /// An empty batch means the wait expired without input.
    fn wait_input(&mut self, timeout: Duration) -> io::Result<ControlFlow<(), Vec<Event>>>;

    /// Restore native terminal state before host teardown can block.
    fn restore(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Own the host for the loop's lifetime, including error and exit paths.
pub fn run_terminal<H: TerminalHost>(host: H, terminal: &mut impl TerminalIo) -> io::Result<()> {
    run_terminal_with_shutdown(host, terminal, || false)
}

/// Honor shutdown between bounded waits and restore the terminal before
/// the host, which may wait on a native backend, is dropped.
pub fn run_terminal_with_shutdown<H: TerminalHost>(
    mut host: H,
    terminal: &mut impl TerminalIo,
    shutdown_requested: impl Fn() -> bool,
) -> io::Result<()> {
    let mut guard = RestoreTerminal {
        terminal,
        active: true,
    };
    let result = run_loop(&mut host, guard.terminal, shutdown_requested);
    guard.active = false;
    let restored = guard.terminal.restore();
    result.and(restored)
}

struct RestoreTerminal<'a, T: TerminalIo> {
    terminal: &'a mut T,
    active: bool,
}

impl<T: TerminalIo> Drop for RestoreTerminal<'_, T> {
    fn drop(&mut self) {
        if self.active {
            let _ = self.terminal.restore();
        }
    }
}

fn run_loop<H: TerminalHost>(
    host: &mut H,
    terminal: &mut impl TerminalIo,
    shutdown_requested: impl Fn() -> bool,
) -> io::Result<()> {
    // Core schedules its next wakeup only when one fires (ADR-044 Am2a):
    // the host fires the first, or no heartbeat ever runs.
    host.dispatch(Event::WakeupFired)
        .map_err(io::Error::other)?;
    while !shutdown_requested() {
        host.poll().map_err(io::Error::other)?;
        let commands = host.take_commands();
        if !commands.is_empty() {
            let rendered = terminal.render(&commands);
            if shutdown_requested() || rendered?.is_break() {
                return Ok(());
            }
        }
        let input = terminal.wait_input(INPUT_WAIT);
        if shutdown_requested() {
            return Ok(());
        }
        match input? {
            ControlFlow::Break(()) => return Ok(()),
            ControlFlow::Continue(events) => {
                for event in events {
                    host.dispatch(event).map_err(io::Error::other)?;
                }
            }
        }
    }
    Ok(())
}
