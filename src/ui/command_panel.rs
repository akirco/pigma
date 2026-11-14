use std::borrow::Cow;

use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    prelude::Widget,
    style::{Modifier, Style},
    widgets::{Clear, Paragraph},
};

use super::{BlockStyle, block::StyledBlock};
use crate::state::{CommandAction, CommandItem, CommandPanel};

pub(super) fn draw(
    f: &mut Frame,
    panel: &CommandPanel,
    default_theme: &str,
    save_on_play: bool,
    bs: &BlockStyle<'_>,
    area: Rect,
) {
    let colors = bs.colors;
    let Some(items) = panel.current_items() else {
        return;
    };

    let title = panel.current_title();
    let inner_height = items.len() as u16 + 2;
    let inner_width = 32u16;

    let popup_area = area.centered(
        Constraint::Length(inner_width),
        Constraint::Length(inner_height),
    );

    let block = StyledBlock::from_color(bs, colors.surface).title(title, colors);
    let inner = block.inner(popup_area);

    f.render_widget(Clear, popup_area);
    block.render(popup_area, f.buffer_mut());

    for (i, item) in items.iter().enumerate() {
        if i >= inner.height as usize {
            break;
        }
        let line_area = Rect {
            y: inner.y + i as u16,
            height: 1,
            ..inner
        };

        let display: Cow<'_, str> = match item {
            CommandItem::Action {
                name,
                action: CommandAction::SwitchTheme(n),
                ..
            } if n == default_theme => Cow::Owned(format!("{} *", name)),
            CommandItem::Action {
                name,
                action: CommandAction::ToggleSaveOnPlay,
                ..
            } => {
                let state = if save_on_play { "ON" } else { "OFF" };
                Cow::Owned(format!("{name}: {state}"))
            }
            CommandItem::Action { name, .. } | CommandItem::SubMenu { name, .. } => {
                Cow::Borrowed(name)
            }
        };

        let prefix = if i == panel.selected {
            crate::icons::PLAY_INDICATOR
        } else {
            " "
        };
        let style = if i == panel.selected {
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors.text)
        };

        f.render_widget(
            Paragraph::new(format!("{prefix} {display}")).style(style),
            line_area,
        );
    }
}
