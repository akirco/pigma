pub fn render_title(template: &str, name: &str, count: usize, total: usize) -> String {
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
