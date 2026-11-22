// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use vauchi_core::{Event, InputMode};
use vauchi_tui::app::App;

// @scenario: generic_presentation_protocol.feature :: Available window drives structural composition
#[test]
fn app_environment_report_says_it_draws_pinned_actions() {
    let event = App::presentation_environment(80, 24);
    let Event::PresentationEnvironmentChanged {
        available_width,
        available_height,
        input_modes,
        pinned_actions,
        ..
    } = event
    else {
        panic!("expected PresentationEnvironmentChanged, got {event:?}");
    };
    assert_eq!((available_width, available_height), (80, 24));
    assert_eq!(input_modes, vec![InputMode::Keyboard]);
    assert!(pinned_actions);
}
