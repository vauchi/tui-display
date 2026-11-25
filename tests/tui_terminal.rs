// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{Event as NativeEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui_core::{backend::TestBackend, terminal::Terminal};
use std::{collections::VecDeque, io, ops::ControlFlow, time::Duration};
use vauchi_contract::Command;
use vauchi_tui_display::{
    host::TerminalIo,
    terminal::{TerminalEvents, TuiTerminal},
};

struct Keys(VecDeque<Option<NativeEvent>>);
impl TerminalEvents for Keys {
    fn next(&mut self, timeout: Duration) -> io::Result<Option<NativeEvent>> {
        assert_eq!(timeout, Duration::from_millis(50));
        Ok(self.0.pop_front().expect("unexpected input wait"))
    }
}

fn commands() -> Vec<Command> {
    let fixture: serde_json::Value =
        serde_json::from_str(vauchi_contract::fixtures::presentation_contract_v1_json()).unwrap();
    serde_json::from_value(fixture["initial_commands"].clone()).unwrap()
}

// @internal — the released display must render real command batches through the host's terminal adapter.
#[test]
fn command_batches_render_into_the_native_terminal_buffer() {
    let mut output = Terminal::new(TestBackend::new(100, 35)).unwrap();
    let mut host = TuiTerminal::new(&mut output, Keys(VecDeque::new()), |effects, _| {
        assert_eq!(effects, &[]);
        Ok(())
    });
    assert_eq!(host.render(&commands()).unwrap(), ControlFlow::Continue(()));
    drop(host);
    let rendered: String = output
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(rendered.contains("Create new identity"), "{rendered}");
}

/// The first wait carries the display's reports: that it answers file
/// picks, and the window. The tests below start after it.
fn past_the_window_report(host: &mut impl TerminalIo) {
    assert!(matches!(
        host.wait_input(Duration::from_millis(50)).unwrap(),
        ControlFlow::Continue(events) if events.len() == 2
    ));
}

// @internal — Core rejects Tab between panes until the window is reported (vauchi/private#1310).
#[test]
fn the_first_wait_reports_the_window_and_a_resize_reports_the_new_one() {
    let mut output = Terminal::new(TestBackend::new(100, 35)).unwrap();
    let mut host = TuiTerminal::new(&mut output, Keys(VecDeque::from([None])), |_, _| Ok(()));
    assert_eq!(host.render(&commands()).unwrap(), ControlFlow::Continue(()));
    let window = |available_width, available_height| {
        vec![
            vauchi_contract::Event::FilePickerCapabilitiesChanged {
                correlated_results: true,
            },
            vauchi_contract::Event::PresentationEnvironmentChanged {
                available_width,
                available_height,
                input_modes: vec![vauchi_contract::InputMode::Keyboard],
                motion: vauchi_contract::MotionPreference::Full,
                color_scheme: None,
                pinned_actions: true,
            },
        ]
    };
    assert_eq!(
        host.wait_input(Duration::from_millis(50)).unwrap(),
        ControlFlow::Continue(window(100, 35))
    );
    assert_eq!(
        host.wait_input(Duration::from_millis(50)).unwrap(),
        ControlFlow::Continue(vec![])
    );
    drop(host);
    output.backend_mut().resize(80, 24);
    let mut host = TuiTerminal::new(&mut output, Keys(VecDeque::new()), |_, _| Ok(()));
    assert_eq!(
        host.wait_input(Duration::from_millis(50)).unwrap(),
        ControlFlow::Continue(window(80, 24))
    );
}

// @internal — Core wiped its data; the old binary ended there too (vauchi/private#1233).
#[test]
fn a_reset_the_host_was_told_of_ends_the_loop() {
    let mut output = Terminal::new(TestBackend::new(100, 35)).unwrap();
    let mut host = TuiTerminal::new(&mut output, Keys(VecDeque::new()), |_, feedback| {
        feedback.quit = true;
        Ok(())
    });

    assert_eq!(host.render(&commands()).unwrap(), ControlFlow::Break(()));
}

// @internal — idle input and key releases must return without forwarding invented Core events; Ctrl-C exits.
#[test]
fn input_waits_are_bounded_and_control_c_quits_without_dispatch() {
    let mut released = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    released.kind = KeyEventKind::Release;
    let input = Keys(VecDeque::from([
        None,
        Some(NativeEvent::Key(released)),
        Some(NativeEvent::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        ))),
    ]));
    let mut output = Terminal::new(TestBackend::new(100, 35)).unwrap();
    let mut host = TuiTerminal::new(&mut output, input, |_, _| Ok(()));
    assert_eq!(host.render(&commands()).unwrap(), ControlFlow::Continue(()));
    past_the_window_report(&mut host);
    assert_eq!(
        host.wait_input(Duration::from_millis(50)).unwrap(),
        ControlFlow::Continue(vec![])
    );
    assert_eq!(
        host.wait_input(Duration::from_millis(50)).unwrap(),
        ControlFlow::Continue(vec![])
    );
    assert_eq!(
        host.wait_input(Duration::from_millis(50)).unwrap(),
        ControlFlow::Break(())
    );
}

// @internal — resize redraws presentation without synthesizing an application action.
#[test]
fn resize_preserves_the_presented_surface() {
    let mut output = Terminal::new(TestBackend::new(100, 35)).unwrap();
    let mut host = TuiTerminal::new(
        &mut output,
        Keys(VecDeque::from([Some(NativeEvent::Resize(100, 35))])),
        |_, _| Ok(()),
    );
    assert_eq!(host.render(&commands()).unwrap(), ControlFlow::Continue(()));
    past_the_window_report(&mut host);
    assert_eq!(
        host.wait_input(Duration::from_millis(50)).unwrap(),
        ControlFlow::Continue(vec![])
    );
    drop(host);
    let rendered: String = output
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(rendered.contains("Create new identity"), "{rendered}");
}

// @internal — dismissing a modal alert must not activate the surface underneath it.
#[test]
fn alert_dismissal_consumes_the_key_before_surface_input_resumes() {
    let enter = NativeEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let input = Keys(VecDeque::from([Some(enter.clone()), Some(enter)]));
    let mut output = Terminal::new(TestBackend::new(100, 35)).unwrap();
    let mut host = TuiTerminal::new(&mut output, input, |_, feedback| {
        feedback.alert = Some(("Alert".into(), "Dismiss before continuing".into()));
        Ok(())
    });
    assert_eq!(host.render(&commands()).unwrap(), ControlFlow::Continue(()));
    past_the_window_report(&mut host);
    assert_eq!(
        host.wait_input(Duration::from_millis(50)).unwrap(),
        ControlFlow::Continue(vec![])
    );
    let fixture: serde_json::Value =
        serde_json::from_str(vauchi_contract::fixtures::presentation_contract_v1_json()).unwrap();
    let surface_id = match &commands()[0] {
        Command::ReplaceSurface { surface } => surface.surface_id.clone(),
        other => panic!("unexpected fixture command: {other:?}"),
    };
    let expected = vec![
        vauchi_contract::Event::SurfaceActivated { surface_id },
        serde_json::from_value(fixture["steps"][0]["event"].clone()).unwrap(),
    ];
    assert_eq!(
        host.wait_input(Duration::from_millis(50)).unwrap(),
        ControlFlow::Continue(expected)
    );
}

struct ClockedIdle(std::rc::Rc<std::cell::Cell<std::time::Instant>>);
impl TerminalEvents for ClockedIdle {
    fn next(&mut self, _: Duration) -> io::Result<Option<NativeEvent>> {
        Ok(None)
    }
    fn now(&self) -> std::time::Instant {
        self.0.get()
    }
}

fn screen_with_a_status_after(elapsed: Duration) -> String {
    let start = std::time::Instant::now();
    let clock = std::rc::Rc::new(std::cell::Cell::new(start));
    let mut output = Terminal::new(TestBackend::new(100, 35)).unwrap();
    let mut host = TuiTerminal::new(&mut output, ClockedIdle(clock.clone()), |_, feedback| {
        feedback.status = Some("Update sent".into());
        Ok(())
    });
    assert_eq!(host.render(&commands()).unwrap(), ControlFlow::Continue(()));
    past_the_window_report(&mut host);
    clock.set(start + elapsed);
    assert_eq!(
        host.wait_input(Duration::from_millis(50)).unwrap(),
        ControlFlow::Continue(vec![])
    );
    drop(host);
    output
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

// @internal — the status box covers the top rows of the screen, where the
// title and the sync chip are; the old binary took it away after 3 s and
// this one kept it until the next status (vauchi/private#1297).
#[test]
fn a_status_leaves_the_screen_after_three_seconds() {
    let shown = screen_with_a_status_after(Duration::from_millis(2900));
    assert!(shown.contains("Update sent"), "{shown}");

    let gone = screen_with_a_status_after(Duration::from_secs(3));
    assert!(!gone.contains("Update sent"), "{gone}");
    assert!(gone.contains("Create new identity"), "{gone}");
}
