use gettextrs::gettext;
use gio::prelude::*;
use gio::SimpleActionGroup;
use libadwaita::prelude::*;
use std::collections::HashSet;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::{
    build_song_menu, fetch_queue_page, labels, DetailsPageModel, GroupButton, QueueMenuEntry,
    SongActions, TrackListModel,
};
use crate::app::models::{QueueRole, SongListModel, SongModel, Track};
use crate::app::state::{
    Device, EntryKey, PlaybackAction, PlaybackState, SelectionAction, SelectionContext,
    SelectionState,
};
use crate::app::{AppModel, Dispatcher};
use crate::feature_flags::{self, FeatureFlag};

pub struct QueueListModel {
    base: DetailsPageModel,
    show_now_playing: bool,
}

impl QueueListModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher, show_now_playing: bool) -> Self {
        Self {
            base: DetailsPageModel::new_without_id(app_model, dispatcher),
            show_now_playing,
        }
    }

    fn queue(&self) -> impl Deref<Target = PlaybackState> + '_ {
        self.base.app_model.map_state(|s| &s.playback)
    }

    pub fn has_queue(&self) -> bool {
        !self.queue().queue_is_empty()
    }

    /// A remote device's queue is read-only.
    pub fn selection_context(&self) -> SelectionContext {
        match self.queue().current_device() {
            Device::Local => SelectionContext::Queue,
            Device::Connect(_) => SelectionContext::ReadOnlyQueue,
        }
    }

    fn listed_keys(&self) -> Vec<EntryKey> {
        let queue = self.queue();
        if self.show_now_playing {
            queue.view_keys().to_vec()
        } else {
            queue.upcoming_keys().to_vec()
        }
    }

    pub fn select_all(&self) {
        let rows: Vec<(String, Track)> = {
            let queue = self.queue();
            self.listed_keys()
                .into_iter()
                .filter_map(|key| Some((key.to_string(), queue.view_track(key)?)))
                .collect()
        };
        self.base
            .dispatcher
            .dispatch(SelectionAction::SelectKeyed(rows).into());
    }

    pub fn play_id(&self, id: &str) {
        let key = self.listed_keys().into_iter().find(|&key| {
            self.queue()
                .view_track(key)
                .is_some_and(|track| track.rri.id == id)
        });
        if let Some(key) = key {
            self.base
                .dispatcher
                .dispatch(PlaybackAction::PlayEntry(key).into());
        }
    }

    fn drop_gap(&self, key: &str, target: &str, after: bool) -> Option<(EntryKey, usize)> {
        let key: EntryKey = key.parse().ok()?;
        let target: EntryKey = target.parse().ok()?;
        let queue = self.queue();
        let queued = queued_rows(&queue);
        let from = queued.iter().position(|k| *k == key)?;
        let gap = match queued.iter().position(|k| *k == target) {
            Some(i) => i + after as usize,
            None if Some(target) == queue.current_key() => after.then_some(0)?,
            None if !after && queue.upcoming_keys().get(queued.len()) == Some(&target) => {
                queued.len()
            }
            None => return None,
        };
        (gap != from && gap != from + 1).then_some((key, gap))
    }

    pub fn toggle_play(&self) {
        self.base.toggle_playback(true, || None);
    }
}

fn queued_rows(queue: &PlaybackState) -> &[EntryKey] {
    if !matches!(queue.current_device(), Device::Local) {
        return &[];
    }
    let upcoming = queue.upcoming_keys();
    let count = upcoming
        .iter()
        .take_while(|k| matches!(k, EntryKey::Queued(_)))
        .count();
    &upcoming[..count]
}

fn queue_entry(row: &SongModel) -> Option<(EntryKey, QueueRole)> {
    Some((row.row_key().parse().ok()?, row.queue_role()?))
}

impl TrackListModel for QueueListModel {
    fn song_list_model(&self) -> SongListModel {
        self.queue().queue_view().clone()
    }

    fn is_paused(&self) -> bool {
        self.base.is_paused()
    }

    fn current_song_id(&self) -> Option<String> {
        self.queue().current_song_id()
    }

    fn hides_row(&self, row: &SongModel) -> bool {
        !self.show_now_playing && row.group().as_deref() == Some(labels::QUEUE_NOW_PLAYING.as_str())
    }

    fn is_row_playing(&self, key: &str) -> bool {
        self.queue()
            .current_key()
            .is_some_and(|k| k.to_string() == key)
    }

    fn autoscroll_to_playing(&self) -> bool {
        false
    }

    fn show_album_column(&self) -> bool {
        true
    }

    fn show_loading_skeleton(&self) -> bool {
        false
    }

    fn deselect_song(&self, key: &str) {
        self.base.deselect_song(key);
    }

    fn selection(&self) -> Option<Box<dyn Deref<Target = SelectionState> + '_>> {
        self.base.selection()
    }

    fn load_more(&self) {
        let query = self.queue().next_query(true);
        if let Some((source, request)) = query {
            fetch_queue_page(&self.base.app_model, &self.base.dispatcher, source, request);
        }
    }

    fn play_song_at(&self, pos: usize, _id: &str) {
        let key = self.queue().view_key(pos);
        if let Some(key) = key {
            self.base
                .dispatcher
                .dispatch(PlaybackAction::PlayEntry(key).into());
        }
    }

    fn select_song(&self, key: &str) {
        let track = key
            .parse::<EntryKey>()
            .ok()
            .and_then(|k| self.queue().view_track(k));
        if let Some(track) = track {
            self.base
                .dispatcher
                .dispatch(SelectionAction::SelectKeyed(vec![(key.to_string(), track)]).into());
        }
    }

    fn enable_selection(&self) -> bool {
        if !feature_flags::is_enabled(FeatureFlag::SelectMode) {
            return false;
        }
        self.base
            .enable_selection_with_context(self.selection_context())
    }

    fn is_song_liked(&self, id: &str) -> bool {
        self.base.is_song_liked(id)
    }

    fn toggle_song_like(&self, id: &str) {
        let songs = self.song_list_model();
        self.base.toggle_song_like(&songs, id);
    }

    fn pinned_song_ids(&self) -> Option<HashSet<String>> {
        self.base.pinned_song_ids()
    }

    fn toggle_song_pin(&self, song: &Track) {
        self.base.toggle_song_pin(song);
    }

    fn skip_explicit(&self) -> bool {
        self.base.skip_explicit()
    }

    fn group_button(&self, group: &str) -> Option<GroupButton> {
        (group == labels::QUEUE_NEXT_IN_QUEUE.as_str()).then(|| GroupButton {
            icon_name: "user-trash-symbolic",
            tooltip: labels::CLEAR_QUEUE.clone(),
            destructive: true,
        })
    }

    fn group_button_clicked(&self, group: &str, source: &gtk::Widget) {
        if group != labels::QUEUE_NEXT_IN_QUEUE.as_str() {
            return;
        }
        let dialog = libadwaita::AlertDialog::new(
            // translators: Title of the dialog asking to confirm clearing the play queue, from its trash button.
            Some(&gettext("Clear the Queue?")),
            // translators: Body of the dialog asking to confirm clearing the play queue.
            Some(&gettext("All tracks added to the queue will be removed.")),
        );
        // translators: Button of the dialog asking to confirm clearing the play queue: keeps the queue.
        dialog.add_response("cancel", &gettext("Cancel"));
        // translators: Button of the dialog asking to confirm clearing the play queue: clears it.
        dialog.add_response("clear", &gettext("Clear"));
        dialog.set_response_appearance("clear", libadwaita::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let dispatcher = self.base.dispatcher.clone();
        dialog.choose(Some(source), gio::Cancellable::NONE, move |response| {
            if response.as_str() == "clear" {
                dispatcher.dispatch(PlaybackAction::ClearQueued.into());
            }
        });
    }

    fn is_reorderable(&self) -> bool {
        true
    }

    fn can_drag_row(&self, key: &str) -> bool {
        let Ok(key) = key.parse::<EntryKey>() else {
            return false;
        };
        queued_rows(&self.queue()).contains(&key) && !self.is_selection_enabled()
    }

    fn can_drop_row(&self, key: &str, target: &str, after: bool) -> bool {
        self.drop_gap(key, target, after).is_some()
    }

    fn drop_row(&self, key: &str, target: &str, after: bool) {
        if let Some((key, to)) = self.drop_gap(key, target, after) {
            self.base
                .dispatcher
                .dispatch(PlaybackAction::MoveQueued { key, to }.into());
        }
    }

    fn actions_for(&self, row: &SongModel, song: &Track) -> Option<SimpleActionGroup> {
        let dispatcher = &self.base.dispatcher;
        let group = SimpleActionGroup::new();
        for a in song.make_artist_actions(dispatcher.clone()) {
            group.add_action(&a);
        }
        group.add_action(&song.make_album_action(dispatcher.clone()));
        group.add_action(&song.make_link_action());
        let actions = match queue_entry(row) {
            Some((key, QueueRole::Queued)) => {
                song.make_queue_entry_actions(key, dispatcher.clone())
            }
            Some((_, QueueRole::Context | QueueRole::Current)) => {
                song.make_queue_actions(dispatcher.clone())
            }
            _ => vec![],
        };
        for a in actions {
            group.add_action(&a);
        }
        Some(group)
    }

    fn menu_for(
        &self,
        row: &SongModel,
        song: &Track,
        liked: bool,
        pinned: Option<bool>,
    ) -> Option<gio::MenuModel> {
        let queue_entry = match queue_entry(row) {
            Some((_, QueueRole::Queued)) => QueueMenuEntry::Queued,
            Some((_, QueueRole::Context | QueueRole::Current)) => QueueMenuEntry::Add,
            _ => QueueMenuEntry::None,
        };
        Some(build_song_menu(
            song,
            true,
            None,
            queue_entry,
            Some(liked),
            pinned,
        ))
    }
}

impl crate::app::ProvidesApi for QueueListModel {
    fn api_service(&self) -> std::sync::Arc<riff_api::ApiService> {
        self.base.app_model.api()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::models::make_track;
    use crate::app::state::AppState;
    use crate::app::{AppAction, SongsSource};
    use std::cell::Cell;
    use std::sync::Arc;

    fn test_api_service() -> Arc<riff_api::ApiService> {
        let token_provider: Arc<dyn riff_api::TokenProvider> = Arc::new(|| None::<String>);
        Arc::new(riff_api::spotify_service(
            token_provider,
            1024 * 1024,
            1024 * 1024,
        ))
    }

    fn make_model(show_now_playing: bool) -> (Rc<AppModel>, Rc<QueueListModel>) {
        let (sender, _receiver) = futures::channel::mpsc::unbounded();
        std::mem::forget(_receiver);
        let app_model = Rc::new(AppModel::new(AppState::new(), test_api_service()));
        let model = Rc::new(QueueListModel::new(
            app_model.clone(),
            Dispatcher::new(sender),
            show_now_playing,
        ));
        (app_model, model)
    }

    #[test]
    fn test_binding_rows_during_state_updates_doesnt_read_state() {
        let (app_model, model) = make_model(true);
        let bound = Rc::new(Cell::new(0));
        let view = app_model.get_state().playback.queue_view().clone();
        {
            let model = model.clone();
            let bound = bound.clone();
            view.connect_items_changed(move |view, position, _, added| {
                for i in position..position + added {
                    let row = view.index_continuous(i as usize).unwrap();
                    let track = row.into_description();
                    let _ = row.row_key();
                    model.actions_for(&row, &track);
                    model.menu_for(&row, &track, false, None);
                    if let Some(group) = row.group() {
                        model.group_button(&group);
                    }
                    bound.set(bound.get() + 1);
                }
            });
        }

        let tracks = ["1", "2", "3", "4"]
            .iter()
            .map(|id| make_track(id))
            .collect();
        let actions: Vec<AppAction> = vec![
            PlaybackAction::LoadContextSongs(SongsSource::Album("a".into()), tracks).into(),
            PlaybackAction::Load("1".to_string()).into(),
            AppAction::QueueTracks {
                tracks: vec![make_track("x"), make_track("x")],
            },
            AppAction::QueueTracks {
                tracks: vec![make_track("y")],
            },
            PlaybackAction::Next.into(),
            PlaybackAction::TrackEnded.into(),
            PlaybackAction::ToggleShuffle.into(),
            PlaybackAction::ToggleRepeat.into(),
            PlaybackAction::Previous.into(),
            PlaybackAction::Stop.into(),
            PlaybackAction::Play.into(),
        ];
        for action in actions {
            app_model.update_state(action);
        }
        app_model.update_state(AppAction::QueueTracks {
            tracks: vec![make_track("z"), make_track("w")],
        });
        let keys = app_model.get_state().playback.view_keys().to_vec();
        let actions: Vec<AppAction> = vec![
            PlaybackAction::RemoveEntries(vec![keys[2]]).into(),
            PlaybackAction::MoveQueued {
                key: keys[3],
                to: 0,
            }
            .into(),
            PlaybackAction::PlayEntry(keys[3]).into(),
            PlaybackAction::ClearQueued.into(),
            PlaybackAction::ReplaceQueue.into(),
        ];
        for action in actions {
            app_model.update_state(action);
        }

        assert!(bound.get() > 0);
    }

    fn row_keys(app_model: &AppModel) -> Vec<String> {
        let state = app_model.get_state();
        state
            .playback
            .view_keys()
            .iter()
            .map(|k| k.to_string())
            .collect()
    }

    #[test]
    fn test_rows_menus_and_drops() {
        let (app_model, model) = make_model(true);
        let album = SongsSource::Album("a".into());
        let tracks = ["1", "2"].iter().map(|id| make_track(id)).collect();
        app_model
            .update_state(PlaybackAction::SetContextName(album.clone(), "An Album".into()).into());
        app_model.update_state(PlaybackAction::LoadContextSongs(album, tracks).into());
        app_model.update_state(PlaybackAction::Load("1".to_string()).into());
        app_model.update_state(AppAction::QueueTracks {
            tracks: ["p", "a", "b", "c"]
                .iter()
                .map(|id| make_track(id))
                .collect(),
        });

        let view = app_model.get_state().playback.queue_view().clone();
        let row = |i| view.index_continuous(i).unwrap();
        let (current, queued, context) = (row(0), row(1), row(5));
        assert!(model.is_row_playing(&current.row_key()));
        let has_action = |row: &SongModel, name: &str| {
            model
                .actions_for(row, &row.into_description())
                .is_some_and(|g| g.lookup_action(name).is_some())
        };
        assert!(has_action(&queued, "dequeue") && !has_action(&queued, "queue"));
        for row in [&current, &context] {
            assert!(has_action(row, "queue") && !has_action(row, "dequeue"));
        }
        let button = model.group_button(&queued.group().unwrap()).unwrap();
        assert_eq!(button.tooltip, *labels::CLEAR_QUEUE);
        assert!(button.destructive);
        for other in [&current, &context] {
            assert_eq!(model.group_button(&other.group().unwrap()), None);
        }

        let rows = row_keys(&app_model);
        let [current, p, a, b, c, context] = &rows[..] else {
            panic!("unexpected rows {:?}", rows);
        };
        assert!(model.can_drag_row(c));
        assert!(!model.can_drag_row(current) && !model.can_drag_row(context));
        let gap = |key, target, after| model.drop_gap(key, target, after).map(|g| g.1);
        assert_eq!(gap(c, current, true), Some(0));
        assert_eq!(gap(c, p, false), Some(0));
        assert_eq!(gap(p, c, true), Some(4));
        assert_eq!(gap(p, context, false), Some(4));
        assert_eq!(gap(b, a, true), None);
        assert_eq!(gap(b, c, false), None);
        assert_eq!(gap(p, current, false), None);
        assert_eq!(gap(p, context, true), None);

        let (key, to) = model.drop_gap(c, p, false).unwrap();
        app_model.update_state(PlaybackAction::MoveQueued { key, to }.into());
        assert_eq!(
            row_keys(&app_model)[1..],
            [c, p, a, b, context].map(|k| k.to_string())
        );

        app_model.update_state(PlaybackAction::Next.into());
        let rows = row_keys(&app_model);
        let [current, p, _a, b, _context] = &rows[..] else {
            panic!("unexpected rows {:?}", rows);
        };
        assert_eq!(current, c);
        assert!(!model.can_drag_row(current));
        assert_eq!(gap(b, p, false), Some(0));
        assert_eq!(gap(b, current, true), Some(0));
        assert_eq!(gap(b, current, false), None);
    }

    #[test]
    fn test_lists_the_current_track_only_with_now_playing() {
        let (sender, mut receiver) = futures::channel::mpsc::unbounded();
        let app_model = Rc::new(AppModel::new(AppState::new(), test_api_service()));
        let tracks = ["1", "2", "3"].iter().map(|id| make_track(id)).collect();
        app_model.update_state(
            PlaybackAction::LoadContextSongs(SongsSource::Album("a".into()), tracks).into(),
        );
        app_model.update_state(PlaybackAction::Load("1".to_string()).into());
        let view = app_model.get_state().playback.queue_view().clone();
        let current = view.index_continuous(0).unwrap();

        for show_now_playing in [true, false] {
            let model = QueueListModel::new(
                app_model.clone(),
                Dispatcher::new(sender.clone()),
                show_now_playing,
            );
            assert_eq!(model.hides_row(&current), !show_now_playing);
            assert!(!model.hides_row(&view.index_continuous(1).unwrap()));
            model.play_id("2");
            let action = receiver.try_next().ok().flatten().expect("an action");
            let AppAction::PlaybackAction(PlaybackAction::PlayEntry(key)) = action else {
                panic!("not PlayEntry: {:?}", action);
            };
            assert_eq!(
                app_model
                    .get_state()
                    .playback
                    .view_track(key)
                    .unwrap()
                    .rri
                    .id,
                "2"
            );
        }
    }
}
