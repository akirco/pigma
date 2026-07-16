use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    prelude::Widget,
    style::{Modifier, Style},
    widgets::{Clear, Paragraph},
};

use crate::state::{App, CommandAction, CommandItem};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let panel = &app.state.command_panel;
    let colors = app.current_theme();
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

    let block = super::create_block(
        title,
        colors,
        app.state.bordered,
        app.state.border_rounded,
        true,
    );
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

        let display = match item {
            CommandItem::Action {
                name,
                action: CommandAction::SwitchTheme(n),
                ..
            } if n == &app.state.current_color_name => {
                format!("{} *", name)
            }
            CommandItem::Action { name, .. } | CommandItem::SubMenu { name, .. } => name.clone(),
        };

        let prefix = if i == panel.selected { "▶ " } else { "  " };
        let style = if i == panel.selected {
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors.text)
        };

        f.render_widget(
            Paragraph::new(format!("{}{}", prefix, display)).style(style),
            line_area,
        );
    }
}
