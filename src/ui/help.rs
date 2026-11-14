use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    prelude::Widget,
    style::Style,
    widgets::{Clear, Paragraph},
};

use super::{BlockStyle, block::StyledBlock};

const HELP_ITEMS: &[(&str, &str)] = &[
    ("Ctrl+C/q", "退出程序"),
    ("Ctrl+P", "命令面板"),
    ("L", "登录网易云"),
    ("w", "清空播放队列"),
    ("?", "帮助"),
    ("Esc", "返回"),
    (SHIFT_TAB, "切换导航区块 / 搜索引擎"),
    ("↑ / ↓ 或 k / j", "上 / 下选择 / 均衡器选频段"),
    ("g / G", "跳转顶部 / 底部"),
    ("Enter", "播放选中 / 进入"),
    ("Space", "播放 / 暂停"),
    ("n / p", "下一首 / 上一首"),
    ("← / →", "平铺卡片左右 / 快退快进 / 均衡器调节"),
    ("+ / -", "音量增大 / 减小"),
    ("[ / ]", "播放速度 -0.1 / +0.1"),
    ("t", "切换变速模式"),
    ("z", "切换导航栏位置"),
    ("m", "循环模式"),
    ("l", "歌词页（再按返回主界面）"),
    ("e", "均衡器页 / 主界面"),
    ("f", "播放队列（再按返回主界面）"),
    ("/", "搜索 / 过滤"),
    ("s", "喜欢选中歌曲"),
    ("S", "喜欢当前播放歌曲"),
    ("a", "添加到队列下一首播放"),
    ("d", "不感兴趣（每日推荐）/ 取消喜欢选中"),
    ("D", "取消喜欢当前播放歌曲"),
    ("c", "窄屏表格：行 / 单元格模式"),
    ("b", "显示 / 隐藏边框"),
    ("u", "上传缓存歌曲"),
    ("r", "手动刷新列表内容 / 均衡器置0"),
];

const SHIFT_TAB: &str = "Tab / \u{21e7}Tab";
const POPUP_WIDTH: u16 = 64;
const POPUP_HEIGHT: u16 = 24;
const KEY_COL_WIDTH: usize = 16;

/// Renders the popup and returns the scroll limit implied by the rendered
/// geometry, for the caller to persist into [`crate::state::HelpState`].
pub(super) fn draw(f: &mut Frame, scroll: usize, bs: &BlockStyle<'_>, area: Rect) -> usize {
    let colors = bs.colors;

    let popup_area = area.centered(
        Constraint::Length(POPUP_WIDTH),
        Constraint::Length(POPUP_HEIGHT),
    );

    let title = format!(
        "{} HELP {}",
        crate::icons::TITLE_START,
        crate::icons::TITLE_END
    );
    let block = StyledBlock::from_color(bs, colors.surface).title(&title, colors);
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
    let max_scroll = HELP_ITEMS.len().saturating_sub(visible);
    let scroll = scroll.min(max_scroll);
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
    max_scroll
}
