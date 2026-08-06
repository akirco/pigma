use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    prelude::Widget,
    style::Style,
    widgets::{Clear, Paragraph},
};

use super::BlockStyle;
use crate::state::App;

const HELP_ITEMS: &[(&str, &str)] = &[
    ("Ctrl+C/q", "退出程序"),
    ("Ctrl+P", "命令面板"),
    ("Ctrl+L", "清空播放队列"),
    ("?", "帮助"),
    ("Esc", "返回"),
    ("Tab / ⇧Tab", "切换导航区块 / 搜索引擎"),
    ("↑ / ↓ 或 k / j", "上 / 下选择"),
    ("g / G", "跳转顶部 / 底部"),
    ("Enter", "播放选中 / 进入"),
    ("Space", "播放 / 暂停"),
    ("n / p", "下一首 / 上一首"),
    ("← / →", "上一列 / 快退，下一列 / 快进"),
    ("m", "循环模式"),
    ("l", "歌词页 / 主界面"),
    ("f", "播放队列 / 主界面"),
    ("/", "搜索 / 过滤"),
    ("s", "喜欢选中歌曲"),
    ("S", "喜欢当前播放歌曲"),
    ("a", "添加到队列下一首播放"),
    ("d", "不喜欢（每日推荐）"),
    ("c", "行 / 单元格模式"),
    ("b", "切换边框模式"),
    ("u", "上传缓存歌曲"),
];

const POPUP_WIDTH: u16 = 64;
const POPUP_HEIGHT: u16 = 24;
const KEY_COL_WIDTH: usize = 16;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let help = &app.state.help;
    let colors = app.current_theme();

    let popup_area = area.centered(
        Constraint::Length(POPUP_WIDTH),
        Constraint::Length(POPUP_HEIGHT),
    );

    let style = BlockStyle {
        colors,
        border: &app.state.border,
        tick: app.state.tick,
    };
    let block = super::create_block_surfaced("\u{25BA} HELP \u{25C4}", &style, false);
    let inner = block.inner(popup_area);

    f.render_widget(Clear, popup_area);
    block.render(popup_area, f.buffer_mut());

    let footer = format!(
        "{:>width$}",
        "↑/↓ 或 j/k 滚动  Esc 关闭",
        width = (POPUP_WIDTH - 4) as usize
    );
    let footer_area = Rect {
        y: inner.y + inner.height.saturating_sub(1),
        height: 1,
        ..inner
    };
    f.render_widget(
        Paragraph::new(footer).style(Style::default().fg(colors.muted)),
        footer_area,
    );

    let visible = (inner.height.saturating_sub(1)) as usize;
    let scroll = help.scroll.min(HELP_ITEMS.len().saturating_sub(visible));
    for (i, (key, desc)) in HELP_ITEMS.iter().enumerate().skip(scroll).take(visible) {
        let line_area = Rect {
            y: inner.y + (i - scroll) as u16,
            height: 1,
            ..inner
        };
        let line = format!("  {:<width$}  {}", key, desc, width = KEY_COL_WIDTH);
        let style = if *key == "?" {
            Style::default().fg(colors.accent)
        } else {
            Style::default().fg(colors.text)
        };
        f.render_widget(Paragraph::new(line).style(style), line_area);
    }
}
