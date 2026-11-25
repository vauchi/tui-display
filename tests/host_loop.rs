// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

//! The terminal host loop against a scripted host: no Core and no hardware,
//! so the loop runs the same on every OS (private#1234).

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    io,
    ops::ControlFlow,
    rc::Rc,
    time::Duration,
};
use vauchi_contract::{Command, Event, SurfaceId};
use vauchi_tui_display::host::{
    TerminalHost, TerminalIo, run_terminal, run_terminal_with_shutdown,
};

/// Each poll hands over the next scripted batch, as an app's bridge does
/// with a repaint or a hardware observation.
#[derive(Default)]
struct Script {
    batches: VecDeque<Vec<Command>>,
    pending: Vec<Command>,
    dispatched: Rc<RefCell<Vec<Event>>>,
    poll_error: bool,
    _teardown: Teardown,
}

/// Records, when the host is dropped, whether the console was already restored.
#[derive(Default)]
struct Teardown {
    restored: Rc<Cell<bool>>,
    restored_when_dropped: Rc<Cell<Option<bool>>>,
}

impl Drop for Teardown {
    fn drop(&mut self) {
        self.restored_when_dropped.set(Some(self.restored.get()));
    }
}

impl Script {
    fn with_batches(batches: Vec<Vec<Command>>) -> Self {
        Self {
            batches: batches.into(),
            ..Self::default()
        }
    }
}

impl TerminalHost for Script {
    type Error = io::Error;

    fn dispatch(&mut self, event: Event) -> Result<(), io::Error> {
        self.dispatched.borrow_mut().push(event);
        Ok(())
    }

    fn poll(&mut self) -> Result<bool, io::Error> {
        if self.poll_error {
            return Err(io::Error::other("hardware helper ended"));
        }
        match self.batches.pop_front() {
            Some(batch) => {
                self.pending.extend(batch);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    fn take_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.pending)
    }
}

struct Console {
    reads: VecDeque<io::Result<ControlFlow<(), Vec<Event>>>>,
    batches: Vec<Vec<Command>>,
    waits: Vec<Duration>,
    stop_after_render: bool,
    restored: Rc<Cell<bool>>,
}

impl Console {
    fn reading(reads: Vec<io::Result<ControlFlow<(), Vec<Event>>>>) -> Self {
        Self {
            reads: reads.into(),
            batches: vec![],
            waits: vec![],
            stop_after_render: false,
            restored: Rc::default(),
        }
    }
}

impl TerminalIo for Console {
    fn render(&mut self, commands: &[Command]) -> io::Result<ControlFlow<()>> {
        self.batches.push(commands.to_vec());
        Ok(if self.stop_after_render {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        })
    }

    fn wait_input(&mut self, timeout: Duration) -> io::Result<ControlFlow<(), Vec<Event>>> {
        self.waits.push(timeout);
        self.reads
            .pop_front()
            .expect("host must stop after input ends")
    }

    fn restore(&mut self) -> io::Result<()> {
        self.restored.set(true);
        Ok(())
    }
}

fn opened() -> Event {
    Event::SurfaceActivated {
        surface_id: SurfaceId::new("welcome").unwrap(),
    }
}

// @internal — waiting for terminal input must not prevent background presentation delivery.
#[test]
fn a_batch_the_host_produces_while_input_is_idle_is_rendered_before_the_next_wait() {
    let host = Script::with_batches(vec![
        vec![Command::PerformNativeBack],
        vec![Command::ResetApplication],
    ]);
    let mut terminal = Console::reading(vec![
        Ok(ControlFlow::Continue(vec![])),
        Ok(ControlFlow::Break(())),
    ]);

    run_terminal(host, &mut terminal).unwrap();

    assert_eq!(
        terminal.batches,
        vec![
            vec![Command::PerformNativeBack],
            vec![Command::ResetApplication]
        ]
    );
    assert_eq!(terminal.waits, vec![Duration::from_millis(50); 2]);
}

// @internal — a terminal host forwards input unchanged and quits without another read.
#[test]
fn input_is_dispatched_unchanged_and_quit_stops_the_loop() {
    let host = Script::default();
    let dispatched = host.dispatched.clone();
    let mut terminal = Console::reading(vec![
        Ok(ControlFlow::Continue(vec![opened()])),
        Ok(ControlFlow::Break(())),
    ]);

    run_terminal(host, &mut terminal).unwrap();

    assert_eq!(*dispatched.borrow(), vec![Event::WakeupFired, opened()]);
    assert_eq!(terminal.waits.len(), 2);
}

// @internal — Core schedules its next wakeup only when one fires, so without
// the host's first one no heartbeat runs: no sync, and a device link stays on
// "Generating link..." (vauchi/private#1297).
#[test]
fn the_host_fires_the_first_wakeup_once_before_it_waits_for_input() {
    let host = Script::default();
    let dispatched = host.dispatched.clone();
    let mut terminal = Console::reading(vec![Ok(ControlFlow::Break(()))]);

    run_terminal(host, &mut terminal).unwrap();

    assert_eq!(*dispatched.borrow(), vec![Event::WakeupFired]);
    assert_eq!(terminal.waits.len(), 1);
}

// @internal — input failure must return promptly instead of spinning or retrying EOF.
#[test]
fn input_failure_ends_the_loop_and_preserves_its_error_kind() {
    let mut terminal = Console::reading(vec![Err(io::ErrorKind::BrokenPipe.into())]);

    let error = run_terminal(Script::default(), &mut terminal).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    assert_eq!(terminal.waits.len(), 1);
}

// @internal — a host that fails (a hardware helper that ended) ends the loop
// with its error and the console restored, never a spin on the failed poll.
#[test]
fn a_failing_host_ends_the_loop_with_its_error_and_the_console_restored() {
    let host = Script {
        poll_error: true,
        ..Script::default()
    };
    let mut terminal = Console::reading(vec![]);
    let restored = terminal.restored.clone();

    let error = run_terminal(host, &mut terminal).unwrap_err();

    assert_eq!(error.to_string(), "hardware helper ended");
    assert_eq!(terminal.waits, vec![]);
    assert!(restored.get());
}

// @internal — native terminal completion must not enter an input wait.
#[test]
fn display_completion_stops_before_reading_input() {
    let host = Script::with_batches(vec![vec![Command::PerformNativeBack]]);
    let mut terminal = Console::reading(vec![]);
    terminal.stop_after_render = true;

    run_terminal(host, &mut terminal).unwrap();

    assert_eq!(terminal.batches, vec![vec![Command::PerformNativeBack]]);
    assert_eq!(terminal.waits, vec![]);
}

// @internal — input read while shutdown was requested is not dispatched: the
// app is ending, and Core must not act on a key it will never answer.
#[test]
fn input_that_arrives_with_a_shutdown_request_is_dropped() {
    let host = Script::default();
    let dispatched = host.dispatched.clone();
    let mut terminal = Console::reading(vec![Ok(ControlFlow::Continue(vec![opened()]))]);
    let waits = Cell::new(0);

    run_terminal_with_shutdown(host, &mut terminal, || {
        waits.set(waits.get() + 1);
        waits.get() >= 2
    })
    .unwrap();

    assert_eq!(*dispatched.borrow(), vec![Event::WakeupFired]);
    assert_eq!(terminal.waits.len(), 1);
}

// @internal — restoring the console must precede host teardown, which may
// wait on a native backend or a helper process.
#[test]
fn terminal_is_restored_before_the_owned_host_is_destroyed() {
    for case in ["quit", "io-error", "panic"] {
        let reads = match case {
            "quit" => vec![Ok(ControlFlow::Break(()))],
            "io-error" => vec![Err(io::ErrorKind::BrokenPipe.into())],
            _ => vec![],
        };
        let mut terminal = Console::reading(reads);
        let teardown = Teardown {
            restored: terminal.restored.clone(),
            restored_when_dropped: Rc::default(),
        };
        let observed = teardown.restored_when_dropped.clone();
        let host = Script {
            _teardown: teardown,
            ..Script::default()
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_terminal(host, &mut terminal)
        }));

        let outcome = match result {
            Ok(Ok(())) => "quit",
            Ok(Err(error)) => {
                assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
                "io-error"
            }
            Err(_) => "panic",
        };
        assert_eq!(outcome, case);
        assert_eq!(observed.get(), Some(true), "{case}");
    }
}
