//! Turning mouse events into actions.
//!
//! Like the key map, this is a pure function of the event and the geometry that
//! was drawn, so every pointer interaction is a table test. The one thing it
//! carries between events is a scrollbar drag: a press grabs the thumb, the
//! drags that follow move it, and the release lets go.
//!
//! Mouse capture is off by default, and this module never asks for it. With it
//! off the terminal keeps its own selection — click-drag to select, Shift-drag
//! where the terminal reserves plain drag for the application — and only the
//! wheel reaches the reader. With it on, the reader sees presses and drags and
//! the terminal's selection usually moves behind Shift; that is a terminal
//! contract, not one this program can enforce.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use reader_app::{Overlay, ReaderState};

use crate::chrome::offset_from_thumb_row;
use crate::frame::{self, CommandBar};
use crate::input::{Action, SCROLL_STEP};
use crate::modals::{self, ModalHit};
use crate::settings_page::SettingsHit;

/// How many lines one wheel notch scrolls.
const WHEEL_STEP: usize = SCROLL_STEP * 3;

/// A scrollbar drag in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollbarDrag {
    /// Rows between the top of the thumb and where it was grabbed, so the thumb
    /// does not jump under the pointer on the first drag event.
    pub grab_offset: usize,
}

/// What the pointer is in the middle of doing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PointerState {
    pub scrollbar_drag: Option<ScrollbarDrag>,
}

impl PointerState {
    /// Whether a drag is in progress.
    #[must_use]
    pub const fn is_dragging(&self) -> bool {
        self.scrollbar_drag.is_some()
    }
}

/// Map one mouse event to an action.
///
/// `area` is the whole frame, the same rectangle [`frame::draw`] was given.
pub fn map_mouse(
    event: MouseEvent,
    area: Rect,
    state: &mut ReaderState,
    command_bar: &CommandBar,
    entries: &[reader_app::OverlayEntry],
    pointer: &mut PointerState,
) -> Action {
    match event.kind {
        // While the palette is open the wheel belongs to its list: that is what
        // the reader is looking at.
        MouseEventKind::ScrollDown if command_bar.active => return Action::CommandNext,
        MouseEventKind::ScrollUp if command_bar.active => return Action::CommandPrevious,
        MouseEventKind::ScrollDown => return Action::ScrollDown(WHEEL_STEP),
        MouseEventKind::ScrollUp => return Action::ScrollUp(WHEEL_STEP),
        MouseEventKind::Up(_) => {
            pointer.scrollbar_drag = None;
            return Action::Ignore;
        }
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {}
        _ => return Action::Ignore,
    }

    let geometry = frame::geometry(area, state, command_bar);
    let dragging = matches!(event.kind, MouseEventKind::Drag(_));

    if let Some(action) = scrollbar_action(event, geometry, state, command_bar, pointer, dragging) {
        return action;
    }
    if dragging {
        // A drag that did not start on the scrollbar is the terminal's business.
        return Action::Ignore;
    }
    overlay_action(event, geometry.body, state, entries)
}

/// A press or drag on the scrollbar column, mapped to a place in the chapter.
fn scrollbar_action(
    event: MouseEvent,
    geometry: frame::FrameGeometry,
    state: &mut ReaderState,
    command_bar: &CommandBar,
    pointer: &mut PointerState,
    dragging: bool,
) -> Option<Action> {
    let column = geometry.scrollbar_column?;
    let body = geometry.body;
    if body.height == 0 {
        return None;
    }
    // A drag keeps following the thumb even when the pointer leaves the column.
    if event.column != column && !(dragging && pointer.is_dragging()) {
        return None;
    }
    if dragging && !pointer.is_dragging() {
        return None;
    }
    if !dragging && (event.row < body.y || event.row >= body.y + body.height) {
        return None;
    }

    let layout = state.layout(frame::footer_height(state, command_bar));
    let total = state.chapter_line_count(layout.content_width, body.height);
    let height = body.height as usize;
    if total <= height {
        return None;
    }
    let metrics = crate::chrome::scrollbar_metrics(total, height, state.block_offset);
    // Rows outside the body during a drag clamp to its ends.
    let row = i32::from(event.row) - i32::from(body.y);

    if dragging {
        let grab = pointer.scrollbar_drag?.grab_offset;
        let offset = offset_from_thumb_row(total, height, row as isize - grab as isize);
        return Some(Action::ScrollTo(offset));
    }

    let row = row.max(0) as usize;
    if row >= metrics.thumb_offset && row < metrics.thumb_offset + metrics.thumb_height {
        // Grabbing the thumb where it already is must not move the page.
        pointer.scrollbar_drag = Some(ScrollbarDrag {
            grab_offset: row - metrics.thumb_offset,
        });
        return Some(Action::Ignore);
    }

    // Clicking the track puts the thumb under the pointer and grabs its middle.
    let grab_offset = metrics.thumb_height / 2;
    pointer.scrollbar_drag = Some(ScrollbarDrag { grab_offset });
    Some(Action::ScrollTo(offset_from_thumb_row(
        total,
        height,
        row as isize - grab_offset as isize,
    )))
}

/// A press inside an open overlay: its close control, its search row, or a row.
fn overlay_action(
    event: MouseEvent,
    body: Rect,
    state: &ReaderState,
    entries: &[reader_app::OverlayEntry],
) -> Action {
    match state.overlay {
        Overlay::None | Overlay::Help => Action::Ignore,
        Overlay::Settings => {
            match crate::settings_page::hit_test(body, entries.len(), event.column, event.row) {
                Some(SettingsHit::Tab(tab)) => Action::SelectSettingsTab(tab),
                Some(SettingsHit::Search) => Action::BeginOverlaySearch,
                Some(SettingsHit::Row(index)) => Action::PointerSelect(index),
                None => Action::Ignore,
            }
        }
        overlay if overlay.is_modal() => {
            match modals::hit_test(
                body,
                entries.len(),
                state.overlay_cursor,
                event.column,
                event.row,
            ) {
                Some(ModalHit::Close) => Action::Dismiss,
                Some(ModalHit::Search) => Action::BeginOverlaySearch,
                Some(ModalHit::Row(index)) => Action::PointerSelect(index),
                None => Action::Ignore,
            }
        }
        _ => side_overlay_action(event, body, state, entries),
    }
}

/// A press inside a side list, whose rows start one row below its border.
fn side_overlay_action(
    event: MouseEvent,
    body: Rect,
    state: &ReaderState,
    entries: &[reader_app::OverlayEntry],
) -> Action {
    let area = frame::side_overlay_area(body);
    if event.column <= area.x
        || event.column >= area.x + area.width - 1
        || event.row <= area.y
        || event.row + 1 >= area.y + area.height
    {
        return Action::Ignore;
    }
    let visible = area.height.saturating_sub(2) as usize;
    if visible == 0 || entries.is_empty() {
        return Action::Ignore;
    }
    let start = crate::chrome::window_start(entries.len(), visible, state.overlay_cursor);
    let index = start + (event.row - area.y - 1) as usize;
    if index >= entries.len() {
        return Action::Ignore;
    }
    Action::PointerSelect(index)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use reader_app::{Overlay, ReaderState};
    use reader_core::{AppSettings, CanonicalBlock, CanonicalBook, CanonicalChapter, RenderMode};
    use reader_storage::Storage;

    use super::{PointerState, map_mouse};
    use crate::frame::{self, CommandBar};
    use crate::input::Action;

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 30,
    };

    fn book() -> CanonicalBook {
        CanonicalBook {
            id: "book".into(),
            title: "Quiet Harbour".into(),
            author: "Author".into(),
            source_path: "/books/quiet.epub".into(),
            import_hash: "hash".into(),
            parser_version: Some(3),
            diagnostics: Vec::new(),
            chapters: vec![CanonicalChapter {
                id: "ch0".into(),
                index: 0,
                title: "Chapter 1".into(),
                href: "ch0.xhtml".into(),
                depth: 0,
                blocks: (0..60)
                    .map(|block| CanonicalBlock::Paragraph {
                        id: format!("b{block}"),
                        text: "The lantern swung once over the quiet harbour at dawn.".into(),
                    })
                    .collect(),
                word_count: 600,
            }],
            cover_path: None,
        }
    }

    fn reader() -> ReaderState {
        let mut state = ReaderState::new(AppSettings {
            render_mode: RenderMode::Plain,
            ..AppSettings::default()
        });
        state.viewport = reader_app::Viewport::new(AREA.width, AREA.height);
        state.current_book = Some(book());
        state
    }

    fn event(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn press(column: u16, row: u16) -> MouseEvent {
        event(MouseEventKind::Down(MouseButton::Left), column, row)
    }

    fn drag(column: u16, row: u16) -> MouseEvent {
        event(MouseEventKind::Drag(MouseButton::Left), column, row)
    }

    fn map(
        mouse: MouseEvent,
        state: &mut ReaderState,
        pointer: &mut PointerState,
        entries: &[reader_app::OverlayEntry],
    ) -> Action {
        map_mouse(mouse, AREA, state, &CommandBar::default(), entries, pointer)
    }

    #[test]
    fn the_wheel_scrolls_in_both_capture_modes() {
        let mut state = reader();
        let mut pointer = PointerState::default();
        for capture in [true, false] {
            state.settings.mouse_capture = capture;
            assert_eq!(
                map(
                    event(MouseEventKind::ScrollDown, 5, 5),
                    &mut state,
                    &mut pointer,
                    &[]
                ),
                Action::ScrollDown(3)
            );
            assert_eq!(
                map(
                    event(MouseEventKind::ScrollUp, 5, 5),
                    &mut state,
                    &mut pointer,
                    &[]
                ),
                Action::ScrollUp(3)
            );
        }
    }

    #[test]
    fn pressing_the_scrollbar_track_jumps_to_the_matching_offset() {
        let mut state = reader();
        let mut pointer = PointerState::default();
        let geometry = frame::geometry(AREA, &state, &CommandBar::default());
        let column = geometry.scrollbar_column.expect("a scrollbar");
        let bottom = geometry.body.y + geometry.body.height - 1;

        let action = map(press(column, bottom), &mut state, &mut pointer, &[]);

        let Action::ScrollTo(offset) = action else {
            panic!("pressing the track should move the page, got {action:?}");
        };
        assert!(offset > 0, "clicking near the end should scroll forward");
        assert!(pointer.is_dragging(), "the press grabs the thumb");
    }

    #[test]
    fn grabbing_the_thumb_where_it_sits_does_not_move_the_page() {
        let mut state = reader();
        let mut pointer = PointerState::default();
        let geometry = frame::geometry(AREA, &state, &CommandBar::default());
        let column = geometry.scrollbar_column.expect("a scrollbar");

        let action = map(
            press(column, geometry.body.y),
            &mut state,
            &mut pointer,
            &[],
        );

        assert_eq!(action, Action::Ignore);
        assert!(pointer.is_dragging());
    }

    #[test]
    fn dragging_the_thumb_moves_the_page_and_releasing_lets_go() {
        let mut state = reader();
        let mut pointer = PointerState::default();
        let geometry = frame::geometry(AREA, &state, &CommandBar::default());
        let column = geometry.scrollbar_column.expect("a scrollbar");
        let body = geometry.body;

        map(press(column, body.y), &mut state, &mut pointer, &[]);
        let action = map(drag(column, body.y + 6), &mut state, &mut pointer, &[]);

        let Action::ScrollTo(offset) = action else {
            panic!("dragging should move the page, got {action:?}");
        };
        assert!(offset > 0, "{offset}");

        map(
            event(MouseEventKind::Up(MouseButton::Left), column, body.y + 6),
            &mut state,
            &mut pointer,
            &[],
        );
        assert!(!pointer.is_dragging(), "releasing clears the drag");
    }

    #[test]
    fn a_drag_that_did_not_start_on_the_scrollbar_is_left_to_the_terminal() {
        let mut state = reader();
        let mut pointer = PointerState::default();
        assert_eq!(
            map(drag(10, 5), &mut state, &mut pointer, &[]),
            Action::Ignore
        );
        assert!(!pointer.is_dragging());
    }

    #[test]
    fn a_press_in_the_reading_column_does_not_intercept_selection() {
        let mut state = reader();
        let mut pointer = PointerState::default();
        assert_eq!(
            map(press(10, 5), &mut state, &mut pointer, &[]),
            Action::Ignore
        );
    }

    #[test]
    fn a_modal_row_close_and_search_all_respond_to_a_click() {
        let mut state = reader();
        state.overlay = Overlay::Keys;
        reader_app::shortcuts_panel::open(&mut state);
        let storage = Storage::open_in_memory().expect("database");
        let entries = reader_app::visible_entries(&state, &storage, 0);
        let mut pointer = PointerState::default();

        let body = frame::geometry(AREA, &state, &CommandBar::default()).body;
        let modal = crate::modals::modal_geometry(body);

        assert_eq!(
            map(
                press(modal.area.x + modal.area.width - 2, modal.area.y),
                &mut state,
                &mut pointer,
                &entries
            ),
            Action::Dismiss
        );
        assert_eq!(
            map(
                press(modal.area.x + 4, modal.area.y + 1),
                &mut state,
                &mut pointer,
                &entries
            ),
            Action::BeginOverlaySearch
        );
        assert_eq!(
            map(
                press(modal.area.x + 4, modal.entries_y + 2),
                &mut state,
                &mut pointer,
                &entries
            ),
            Action::PointerSelect(2)
        );
    }

    #[test]
    fn a_side_list_row_responds_to_a_click() {
        let mut state = reader();
        state.overlay = Overlay::Chapters;
        let storage = Storage::open_in_memory().expect("database");
        let entries = reader_app::visible_entries(&state, &storage, 0);
        let mut pointer = PointerState::default();

        let body = frame::geometry(AREA, &state, &CommandBar::default()).body;
        let area = frame::side_overlay_area(body);

        assert_eq!(
            map(
                press(area.x + 2, area.y + 1),
                &mut state,
                &mut pointer,
                &entries
            ),
            Action::PointerSelect(0)
        );
        assert_eq!(
            map(
                press(area.x, area.y + 1),
                &mut state,
                &mut pointer,
                &entries
            ),
            Action::Ignore,
            "the border is not a row"
        );
    }

    #[test]
    fn the_settings_page_responds_to_its_tabs_search_and_rows() {
        let mut state = reader();
        state.overlay = Overlay::Settings;
        reader_app::settings_panel::open(&mut state);
        let storage = Storage::open_in_memory().expect("database");
        let entries = reader_app::visible_entries(&state, &storage, 0);
        let mut pointer = PointerState::default();

        let body = frame::geometry(AREA, &state, &CommandBar::default()).body;

        // " Themes " starts at the content column; " Reading " follows it.
        assert_eq!(
            map(
                press(body.x + 2, body.y + 2),
                &mut state,
                &mut pointer,
                &entries
            ),
            Action::SelectSettingsTab(reader_core::SettingsTab::Themes)
        );
        assert_eq!(
            map(
                press(body.x + 13, body.y + 2),
                &mut state,
                &mut pointer,
                &entries
            ),
            Action::SelectSettingsTab(reader_core::SettingsTab::Reading)
        );
        assert_eq!(
            map(
                press(body.x + 4, body.y + 5),
                &mut state,
                &mut pointer,
                &entries
            ),
            Action::BeginOverlaySearch
        );
        assert_eq!(
            map(
                press(body.x + 4, body.y + 9),
                &mut state,
                &mut pointer,
                &entries
            ),
            Action::PointerSelect(1)
        );
        assert_eq!(
            map(
                press(body.x + 4, body.y + 20),
                &mut state,
                &mut pointer,
                &entries
            ),
            Action::Ignore,
            "past the last setting there is nothing to select"
        );
    }

    #[test]
    fn a_click_outside_an_overlay_does_nothing() {
        let mut state = reader();
        state.overlay = Overlay::Keys;
        let storage = Storage::open_in_memory().expect("database");
        let entries = reader_app::visible_entries(&state, &storage, 0);
        let mut pointer = PointerState::default();

        assert_eq!(
            map(press(1, 2), &mut state, &mut pointer, &entries),
            Action::Ignore
        );
    }
}
