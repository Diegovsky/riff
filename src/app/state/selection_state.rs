use std::borrow::Cow;
use std::collections::HashSet;

use crate::app::models::Track;
use crate::app::state::{AppAction, AppEvent, UpdatableState};

#[derive(Clone, Debug)]
pub enum SelectionAction {
    Select(Vec<Track>),
    SelectKeyed(Vec<(String, Track)>),
    Deselect(Vec<String>),
    Clear,
}

impl From<SelectionAction> for AppAction {
    fn from(selection_action: SelectionAction) -> Self {
        Self::SelectionAction(selection_action)
    }
}

#[derive(Clone, Debug)]
pub enum SelectionEvent {
    // Mode means selection active or not
    SelectionModeChanged(bool),
    SelectionChanged,
}

impl From<SelectionEvent> for AppEvent {
    fn from(selection_event: SelectionEvent) -> Self {
        Self::SelectionEvent(selection_event)
    }
}

#[derive(Debug, Clone)]
pub enum SelectionContext {
    ReadOnlyQueue,
    Queue,
    Playlist,
    EditablePlaylist(String),
    SavedTracks,
    Default,
}

pub struct SelectionState {
    selected_songs: Vec<(String, Track)>,
    selected_keys: HashSet<String>,
    selection_active: bool,
    pub context: SelectionContext,
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            selected_songs: Default::default(),
            selected_keys: Default::default(),
            selection_active: false,
            context: SelectionContext::Default,
        }
    }
}

impl SelectionState {
    fn select(&mut self, key: String, song: Track) -> bool {
        let selected = self.selected_keys.contains(&key);
        if !selected {
            self.selected_keys.insert(key.clone());
            self.selected_songs.push((key, song));
        }
        !selected
    }

    fn deselect(&mut self, key: &str) -> bool {
        self.selected_songs.retain(|(k, _)| k != key);
        self.selected_keys.remove(key)
    }

    pub fn set_mode(&mut self, context: Option<SelectionContext>) -> Option<bool> {
        let currently_active = self.selection_active;
        match (currently_active, context) {
            (false, Some(context)) => {
                *self = Default::default();
                self.selection_active = true;
                self.context = context;
                Some(true)
            }
            (true, None) => {
                *self = Default::default();
                self.selection_active = false;
                Some(false)
            }
            _ => None,
        }
    }

    pub fn is_selection_enabled(&self) -> bool {
        self.selection_active
    }

    pub fn is_song_selected(&self, key: &str) -> bool {
        self.selected_keys.contains(key)
    }

    pub fn count(&self) -> usize {
        self.selected_keys.len()
    }

    // Clears (!) the selection, returns associated memory
    pub fn take_selection(&mut self) -> Vec<Track> {
        self.take_keyed_selection()
            .into_iter()
            .map(|(_, track)| track)
            .collect()
    }

    pub fn take_keyed_selection(&mut self) -> Vec<(String, Track)> {
        std::mem::take(self).selected_songs
    }

    fn select_rows(
        &mut self,
        rows: impl IntoIterator<Item = (String, Track)>,
    ) -> Vec<SelectionEvent> {
        let changed = rows.into_iter().fold(false, |result, (key, track)| {
            self.select(key, track) || result
        });
        if changed {
            vec![SelectionEvent::SelectionChanged]
        } else {
            vec![]
        }
    }

    // Just have a look at the selection without changing it
    pub fn peek_selection(&self) -> impl Iterator<Item = &'_ Track> {
        self.selected_songs.iter().map(|(_, track)| track)
    }
}

impl UpdatableState for SelectionState {
    type Action = SelectionAction;
    type Event = SelectionEvent;

    fn update_with(&mut self, action: Cow<Self::Action>) -> Vec<Self::Event> {
        match action.into_owned() {
            SelectionAction::Select(tracks) => {
                self.select_rows(tracks.into_iter().map(|t| (t.rri.id.clone(), t)))
            }
            SelectionAction::SelectKeyed(rows) => self.select_rows(rows),
            SelectionAction::Deselect(ids) => {
                let changed = ids
                    .iter()
                    .fold(false, |result, id| self.deselect(id) || result);
                if changed {
                    vec![SelectionEvent::SelectionChanged]
                } else {
                    vec![]
                }
            }
            SelectionAction::Clear => {
                self.take_selection();
                vec![SelectionEvent::SelectionModeChanged(false)]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::models::make_track;

    fn update(state: &mut SelectionState, action: SelectionAction) -> Vec<SelectionEvent> {
        state.update_with(Cow::Owned(action))
    }

    #[test]
    fn test_selection_by_id_and_by_key() {
        let mut state = SelectionState::default();
        state.set_mode(Some(SelectionContext::Default));
        let events = update(
            &mut state,
            SelectionAction::Select(vec![make_track("a"), make_track("b"), make_track("a")]),
        );
        assert_eq!(events.len(), 1);
        assert_eq!(state.count(), 2);
        assert!(state.is_song_selected("a"));
        assert_eq!(state.take_selection().len(), 2);
        assert!(!state.is_selection_enabled());

        state.set_mode(Some(SelectionContext::Queue));
        update(
            &mut state,
            SelectionAction::SelectKeyed(vec![
                ("k1".to_string(), make_track("a")),
                ("k2".to_string(), make_track("a")),
            ]),
        );
        assert_eq!(state.count(), 2);
        assert!(state.is_song_selected("k1"));
        assert!(!state.is_song_selected("a"));
        update(
            &mut state,
            SelectionAction::Deselect(vec!["k1".to_string()]),
        );
        let taken = state.take_keyed_selection();
        assert_eq!(taken.len(), 1);
        assert_eq!(
            (taken[0].0.as_str(), taken[0].1.rri.id.as_str()),
            ("k2", "a")
        );
    }
}
