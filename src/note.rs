use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{json::Json, App};

/// A global, project-independent note. `body` holds Markdown source that
/// the preview view renders via `markdown::render_markdown`.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct Note {
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub created_at: Option<u64>,
    #[serde(default)]
    pub updated_at: Option<u64>,
}

impl Note {
    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    pub fn load_items(app: &mut App, items: &mut Vec<ListItem>) {
        items.clear();

        for note in app.notes.iter() {
            // First non-empty body line as a dim one-line snippet
            let snippet: String = note
                .body
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .chars()
                .take(40)
                .collect();

            let mut spans = vec![Span::raw(note.title.clone())];
            if !snippet.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", snippet),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            items.push(ListItem::from(Line::from(spans)))
        }

        let selected = app
            .selected_note_index
            .selected()
            .unwrap_or(0)
            .min(app.notes.len().saturating_sub(1));
        app.selected_note_index.select(Some(selected));
    }

    pub fn get_current(app: &mut App) -> &Note {
        &app.notes[app.selected_note_index.selected().unwrap()]
    }

    pub fn create(app: &mut App, items: &mut Vec<ListItem>, value: &str) {
        if value.is_empty() {
            return;
        }

        app.notes.push(Note {
            title: value.to_string(),
            body: String::new(),
            created_at: Some(Self::current_timestamp()),
            updated_at: None,
        });

        Json::write_notes(app.notes.clone());
        Note::load_items(app, items)
    }

    pub fn rename(app: &mut App, items: &mut Vec<ListItem>, value: &str) {
        app.notes[app.selected_note_index.selected().unwrap()].title = value.to_string();

        Json::write_notes(app.notes.clone());
        Note::load_items(app, items)
    }

    pub fn update_body(app: &mut App, value: &str) {
        let note = &mut app.notes[app.selected_note_index.selected().unwrap()];
        note.body = value.to_string();
        note.updated_at = Some(Self::current_timestamp());

        Json::write_notes(app.notes.clone());
    }

    pub fn delete(app: &mut App, items: &mut Vec<ListItem>) {
        app.notes
            .remove(app.selected_note_index.selected().unwrap());

        Json::write_notes(app.notes.clone());
        Note::load_items(app, items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{make_app, setup_temp_config, ENV_LOCK};

    fn make_note(title: &str, body: &str) -> Note {
        Note {
            title: title.to_string(),
            body: body.to_string(),
            created_at: Some(1_700_000_000),
            updated_at: None,
        }
    }

    #[test]
    fn create_appends_note_and_persists() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = make_app(vec![]);
        let mut items = vec![];

        Note::create(&mut app, &mut items, "shopping");

        assert_eq!(app.notes.len(), 1);
        assert_eq!(app.notes[0].title, "shopping");
        assert!(app.notes[0].created_at.is_some());
        assert_eq!(items.len(), 1);
        assert_eq!(Json::read_notes(), app.notes);
    }

    #[test]
    fn create_with_empty_value_is_a_no_op() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = make_app(vec![]);
        let mut items = vec![];

        Note::create(&mut app, &mut items, "");

        assert!(app.notes.is_empty());
    }

    #[test]
    fn rename_updates_selected_note() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = make_app(vec![]);
        app.notes = vec![make_note("old", "")];
        let mut items = vec![];

        Note::rename(&mut app, &mut items, "new");

        assert_eq!(app.notes[0].title, "new");
        assert_eq!(Json::read_notes()[0].title, "new");
    }

    #[test]
    fn update_body_sets_body_and_updated_at() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = make_app(vec![]);
        app.notes = vec![make_note("n", "")];

        Note::update_body(&mut app, "# Hello\nworld");

        assert_eq!(app.notes[0].body, "# Hello\nworld");
        assert!(app.notes[0].updated_at.is_some());
        assert_eq!(Json::read_notes()[0].body, "# Hello\nworld");
    }

    #[test]
    fn delete_removes_selected_note() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = make_app(vec![]);
        app.notes = vec![make_note("n1", ""), make_note("n2", "")];
        app.selected_note_index.select(Some(1));
        let mut items = vec![];

        Note::delete(&mut app, &mut items);

        assert_eq!(app.notes.len(), 1);
        assert_eq!(app.notes[0].title, "n1");
        assert_eq!(Json::read_notes().len(), 1);
    }

    #[test]
    fn load_items_shows_first_body_line_as_snippet() {
        let mut app = make_app(vec![]);
        app.notes = vec![make_note("n", "\n# Title\nsecond line")];
        let mut items = vec![];

        Note::load_items(&mut app, &mut items);

        assert_eq!(items.len(), 1);
    }

    #[test]
    fn load_items_clamps_selection_into_bounds() {
        let mut app = make_app(vec![]);
        app.notes = vec![make_note("only", "")];
        app.selected_note_index.select(Some(5));
        let mut items = vec![];

        Note::load_items(&mut app, &mut items);

        assert_eq!(app.selected_note_index.selected(), Some(0));
    }
}
