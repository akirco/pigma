use crate::event::AppEvent;
use crate::state::App;

pub(super) fn navigate_nav_up(app: &mut App) {
    let nav = &mut app.state.navigation.nav;
    if nav.sections.is_empty() {
        return;
    }
    let focus = nav.focus_section;
    let len = nav.sections[focus].items.len();
    if len == 0 {
        return;
    }
    let current = nav.section_states[focus].selected().unwrap_or(0);
    if current > 0 {
        nav.section_states[focus].select(Some(current - 1));
    } else {
        let section_count = nav.sections.len();
        let prev_section = (focus + section_count - 1) % section_count;
        if nav.sections[prev_section].items.is_empty() {
            return;
        }
        nav.section_states[focus].select(Some(0));
        nav.focus_section = prev_section;
        let prev_len = nav.sections[prev_section].items.len();
        nav.section_states[prev_section].select(Some(prev_len - 1));
    }
    emit_nav_select(app);
}

pub(super) fn navigate_nav_down(app: &mut App) {
    let nav = &mut app.state.navigation.nav;
    if nav.sections.is_empty() {
        return;
    }
    let focus = nav.focus_section;
    let len = nav.sections[focus].items.len();
    if len == 0 {
        return;
    }
    let current = nav.section_states[focus].selected().unwrap_or(0);
    if current + 1 < len {
        nav.section_states[focus].select(Some(current + 1));
    } else {
        let next_section = (focus + 1) % nav.sections.len();
        if nav.sections[next_section].items.is_empty() {
            return;
        }
        nav.section_states[focus].select(Some(0));
        nav.focus_section = next_section;
        nav.section_states[next_section].select(Some(0));
    }
    emit_nav_select(app);
}

pub(super) fn emit_nav_select(app: &mut App) {
    let nav = &app.state.navigation.nav;
    if nav.sections.is_empty() {
        return;
    }
    let focus = nav.focus_section;
    if let Some(selected) = nav.section_states[focus].selected()
        && let Some(api) = nav.sections[focus]
            .items
            .get(selected)
            .and_then(|i| i.api.as_ref())
    {
        app.state.events.send(AppEvent::NavSelect(api.clone()));
    }
}
