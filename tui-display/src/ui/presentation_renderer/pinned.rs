// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui_core::layout::{Constraint, Layout, Rect};
use ratatui_core::terminal::Frame;
use ratatui_core::text::Line;
use ratatui_widgets::block::Block;
use ratatui_widgets::borders::Borders;
use ratatui_widgets::paragraph::{Paragraph, Wrap};
use vauchi_contract::ActionSpec;

use super::{PresentationState, action_style};

pub(super) fn height(state: &PresentationState, width: u16) -> u16 {
    widgets(state, usize::MAX)
        .iter()
        .fold(0u16, |height, widget| {
            height.saturating_add(rows(widget, width))
        })
}

pub(super) fn draw(frame: &mut Frame, area: Rect, state: &PresentationState, selected: usize) {
    let widgets = widgets(state, selected);
    let constraints = widgets
        .iter()
        .map(|widget| Constraint::Length(rows(widget, area.width)));
    let areas = Layout::vertical(constraints).split(area);
    for (widget, area) in widgets.into_iter().zip(areas.iter()) {
        frame.render_widget(widget, *area);
    }
}

fn widgets(state: &PresentationState, selected: usize) -> Vec<Paragraph<'_>> {
    let Some(bar) = state.context_bar() else {
        return Vec::new();
    };
    let start = state.context_actions().len() - bar.pinned.len();
    bar.pinned
        .iter()
        .enumerate()
        .map(|(index, action)| widget(action, start + index == selected))
        .collect()
}

fn widget(action: &ActionSpec, selected: bool) -> Paragraph<'_> {
    Paragraph::new(Line::styled(
        action.label.as_str(),
        action_style(action, selected, false),
    ))
    .wrap(Wrap { trim: false })
    .block(Block::default().borders(Borders::ALL))
}

fn rows(widget: &Paragraph<'_>, width: u16) -> u16 {
    u16::try_from(widget.line_count(width)).unwrap_or(u16::MAX)
}
