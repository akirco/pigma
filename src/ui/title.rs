use std::sync::Arc;

use crate::state::NavigationState;

/// Content-panel title for the current navigation item.
///
/// The rendered `Arc<String>` is cached and reused while the focus, selection,
/// content generation and item count are unchanged, so the hot draw path does
/// not re-render (or re-allocate) the title every frame.
pub(super) fn content_title(nst: &NavigationState) -> Arc<String> {
    let focus = nst.nav.focus_section;
    let selected = nst.nav.selected_index();
    let generation = nst.generation;
    let count = nst.content.len();
    let cached = nst.title_cache.borrow();
    if let Some((ref title, f, s, g, c)) = *cached
        && f == focus
        && s == selected
        && g == generation
        && c == count
    {
        return Arc::clone(title);
    }
    drop(cached);

    let current_item = nst.nav.selected_item();
    let name = current_item
        .map(|item| item.name.as_str())
        .unwrap_or("SONGS");
    let total = nst
        .pagination
        .as_ref()
        .map(|p| p.total as usize)
        .unwrap_or(count);
    // When content is paged (total known), show `count/total` like the music
    // cloud drive; non-paged content shows only the loaded count. Each nav item
    // can also override this via `title_template`.
    let show_total = nst.pagination.as_ref().is_some_and(|p| p.total > 0);
    let template = current_item
        .and_then(|item| item.title_template.as_deref())
        .unwrap_or(if show_total {
            "\u{25BA} {name} ({count}/{total}) \u{25C4}"
        } else {
            "\u{25BA} {name} ({count}) \u{25C4}"
        });
    let title = Arc::new(render_title(template, name, count, total));
    *nst.title_cache.borrow_mut() = Some((Arc::clone(&title), focus, selected, generation, count));
    title
}

pub(super) fn render_title(template: &str, name: &str, count: usize, total: usize) -> String {
    if !template.contains('{') {
        return template.to_owned();
    }
    template
        .replace("{name}", name)
        .replace("{count}", &count.to_string())
        .replace("{total}", &total.to_string())
}

#[cfg(test)]
mod tests {
    use super::render_title;

    #[test]
    fn title_with_count_suffix() {
        assert_eq!(
            render_title("每日推荐 ({count})", "每日推荐", 12, 0),
            "每日推荐 (12)"
        );
    }

    #[test]
    fn title_name_then_count() {
        assert_eq!(render_title("{name} ({count})", "歌单", 3, 0), "歌单 (3)");
    }

    #[test]
    fn title_no_placeholder() {
        assert_eq!(render_title("SONGS", "x", 0, 0), "SONGS");
    }

    #[test]
    fn title_adjacent_placeholders() {
        assert_eq!(render_title("{name}{count}", "A", 5, 0), "A5");
    }

    #[test]
    fn title_total_placeholder() {
        assert_eq!(
            render_title("{name} ({count}/{total})", "云盘", 50, 137),
            "云盘 (50/137)"
        );
    }
}
