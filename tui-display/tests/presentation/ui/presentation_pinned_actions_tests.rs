// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use super::presentation_input::{InteractionState, KeyOutcome};
use super::presentation_protocol::PresentationState;
use super::presentation_renderer;
use super::presentation_renderer_tests::{action, highlighted_lines, row, titled_surface};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_core::{backend::TestBackend, buffer::Buffer, style::Modifier, terminal::Terminal};
use vauchi_contract::{
    AccessibilitySpec, BindingId, Command, ContextBar, Event, InteractionId, PresentationListStyle,
    PresentationNode, SurfaceId,
};

fn pinned_bar() -> ContextBar {
    ContextBar {
        back: Some(action("back", "Back")),
        navigation: Some(action("navigation", "Navigate")),
        primary: Some(action("primary", "Continue")),
        secondary: Some(action("secondary", "More")),
        info: Some(action("info", "Info")),
        pinned: vec![
            action("answer-yes", "Yes, reconnect Alice"),
            action("answer-no", "No, stop the exchange"),
        ],
    }
}

fn state_with_bar(bar: ContextBar) -> PresentationState {
    let mut surface = titled_surface("question", "Question");
    surface.nodes = vec![PresentationNode::List {
        id: BindingId::new("content").unwrap(),
        label: None,
        style: PresentationListStyle::Rows,
        rows: (0..40)
            .map(|index| row(&format!("Content {index:02}"), Some(action("row", "Open"))))
            .collect(),
        searchable: false,
        search_placeholder: None,
        paging: None,
        accessibility: AccessibilitySpec::label("Content"),
    }];
    let mut state = PresentationState::default();
    state.apply(&[
        Command::ReplaceSurface { surface },
        Command::SetContextBar {
            surface_id: SurfaceId::new("question").unwrap(),
            revision: 1,
            bar: Box::new(bar),
        },
    ]);
    state
}

fn render(state: &PresentationState, width: u16, selection: Option<usize>) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, 18)).unwrap();
    terminal
        .draw(|frame| presentation_renderer::draw(frame, frame.area(), state, 0, selection))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn lines(buffer: &Buffer) -> Vec<String> {
    buffer
        .content
        .chunks(usize::from(buffer.area.width))
        .map(|cells| cells.iter().map(|cell| cell.symbol()).collect())
        .collect()
}

fn line_of(lines: &[String], label: &str) -> usize {
    lines
        .iter()
        .position(|line| line.contains(label))
        .unwrap_or_else(|| panic!("missing {label:?} in {lines:?}"))
}

// @scenario: generic_presentation_protocol.feature :: Every shell renders the same prepared presentation
#[test]
fn pinned_actions_stay_under_scrolled_content_in_core_order_above_primary() {
    let state = state_with_bar(pinned_bar());
    let top = render(&state, 80, Some(0));
    let scrolled = render(&state, 80, Some(39));
    let top_lines = lines(&top);
    let scrolled_lines = lines(&scrolled);
    let first = line_of(&top_lines, "Yes, reconnect Alice");
    let second = line_of(&top_lines, "No, stop the exchange");
    let primary = line_of(&top_lines, "[ Continue ]");
    assert!(line_of(&top_lines, "Content 00") < first);
    assert!(line_of(&scrolled_lines, "Content 39") < first);
    assert!(first < second && second < primary);
    assert_eq!(line_of(&scrolled_lines, "Yes, reconnect Alice"), first);
    assert_eq!(line_of(&scrolled_lines, "No, stop the exchange"), second);
    assert_eq!(&top_lines[first - 1..], &scrolled_lines[first - 1..]);
    for (label, y) in [
        ("Yes, reconnect Alice", first),
        ("No, stop the exchange", second),
    ] {
        assert_eq!(
            top_lines.iter().filter(|line| line.contains(label)).count(),
            1
        );
        assert_eq!(top[(0, y as u16)].symbol(), "│");
        assert_eq!(top[(79, y as u16)].symbol(), "│");
        let x = top_lines[y].find(label).unwrap();
        assert_eq!(top[(x as u16, y as u16)].modifier, Modifier::empty());
    }
}

// @scenario: generic_presentation_protocol.feature :: Every shell renders the same prepared presentation
#[test]
fn pinned_labels_wrap_and_an_empty_group_reserves_no_rows() {
    let mut bar = pinned_bar();
    bar.back = None;
    bar.navigation = None;
    bar.secondary = None;
    bar.info = None;
    let wrapped = lines(&render(&state_with_bar(bar.clone()), 24, None));
    let first = line_of(&wrapped, "Yes, reconnect Alice");
    let second = line_of(&wrapped, "No, stop the exchange");
    assert!(first < second);

    bar.pinned[0].label = "Yes, reconnect Alice and keep the existing contact".into();
    let long = lines(&render(&state_with_bar(bar.clone()), 24, None));
    let joined = long
        .iter()
        .map(|line| line.trim_matches(['│', ' ']))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        joined.contains("Yes, reconnect Alice and keep the existing contact"),
        "{long:?}"
    );

    bar.pinned.clear();
    let empty = lines(&render(&state_with_bar(bar), 24, None));
    assert_eq!(
        empty
            .iter()
            .filter(|line| line.contains("reconnect") || line.contains("stop the exchange"))
            .count(),
        0
    );
    assert_eq!(line_of(&empty, "[ Continue ]"), 16);
    assert_eq!(line_of(&empty, "Content 12"), 13);
}

fn key(interaction: &mut InteractionState, state: &PresentationState, code: KeyCode) -> KeyOutcome {
    interaction.key_outcome(state, KeyEvent::new(code, KeyModifiers::NONE))
}

fn activation(id: &str) -> KeyOutcome {
    KeyOutcome::Events(vec![
        Event::SurfaceActivated {
            surface_id: SurfaceId::new("question").unwrap(),
        },
        Event::ActionActivated {
            surface_id: SurfaceId::new("question").unwrap(),
            interaction_id: InteractionId::new(id).unwrap(),
        },
    ])
}

// @scenario: generic_presentation_protocol.feature :: User interaction returns as an opaque event
#[test]
fn tab_reaches_context_and_pinned_actions_in_core_order() {
    let state = state_with_bar(pinned_bar());
    let mut interaction = InteractionState::default();
    for id in [
        "navigation",
        "primary",
        "secondary",
        "info",
        "answer-yes",
        "answer-no",
        "back",
    ] {
        assert_eq!(
            key(&mut interaction, &state, KeyCode::Tab),
            KeyOutcome::Consumed
        );
        assert_eq!(
            key(&mut interaction, &state, KeyCode::Enter),
            activation(id)
        );
        let mut terminal = Terminal::new(TestBackend::new(80, 18)).unwrap();
        terminal
            .draw(|frame| {
                presentation_renderer::draw(
                    frame,
                    frame.area(),
                    &state,
                    interaction.selected_context(),
                    None,
                )
            })
            .unwrap();
        let label = state.context_actions()[interaction.selected_context()]
            .label
            .clone();
        assert_eq!(highlighted_lines(terminal.backend().buffer()).len(), 1);
        assert!(highlighted_lines(terminal.backend().buffer())[0].contains(&label));
    }
}

// @scenario: generic_presentation_protocol.feature :: User interaction returns as an opaque event
#[test]
fn tab_leaves_scrolled_content_and_disabled_pinned_answers_do_not_dispatch() {
    let mut bar = pinned_bar();
    bar.pinned[1].enabled = false;
    let state = state_with_bar(bar);
    let mut interaction = InteractionState::default();
    assert_eq!(
        key(&mut interaction, &state, KeyCode::End),
        KeyOutcome::Consumed
    );
    for _ in 0..6 {
        assert_eq!(
            key(&mut interaction, &state, KeyCode::Tab),
            KeyOutcome::Consumed
        );
    }
    assert_eq!(interaction.selected_surface_target(&state), None);
    assert_eq!(
        key(&mut interaction, &state, KeyCode::Enter),
        KeyOutcome::Consumed
    );
}

// @scenario: generic_presentation_protocol.feature :: User interaction returns as an opaque event
#[test]
fn activating_the_second_pinned_answer_without_scrolling_dispatches_its_interaction_id() {
    let mut without_primary = pinned_bar();
    without_primary.primary = None;
    for (bar, tabs) in [(pinned_bar(), 6), (without_primary, 5)] {
        let state = state_with_bar(bar);
        let mut interaction = InteractionState::default();
        for _ in 0..tabs {
            assert_eq!(
                key(&mut interaction, &state, KeyCode::Tab),
                KeyOutcome::Consumed
            );
        }
        assert_eq!(
            key(&mut interaction, &state, KeyCode::Enter),
            activation("answer-no")
        );
    }
}

// @scenario: generic_presentation_protocol.feature :: Every shell renders the same prepared presentation
#[test]
fn a_bar_without_pinned_actions_draws_no_pinned_group() {
    let mut bar = pinned_bar();
    bar.pinned.clear();
    let rendered = lines(&render(&state_with_bar(bar), 80, None));
    assert_eq!(
        rendered
            .iter()
            .filter(|line| line.contains("reconnect") || line.contains("stop the exchange"))
            .count(),
        0
    );
    assert_eq!(line_of(&rendered, "[ Continue ]"), 16);
    assert_eq!(line_of(&rendered, "Content 12"), 13);
}
