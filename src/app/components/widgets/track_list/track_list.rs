use gio::SimpleActionGroup;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use riff_api::ApiService;

use crate::app::components::utils::{ancestor, set_css_class, AnimatorDefault};
use crate::app::components::{
    labels, Component, DiscHeaderRow, EventListener, RowOptions, TrackRow, ROW_HEIGHT_PX,
};
use crate::app::models::{SongListModel, SongModel, SongState, Track, TrackExt};
use crate::app::state::{BrowserEvent, PlaybackEvent, SelectionEvent, SelectionState};
use crate::app::{AppEvent, ProvidesApi};

const SKELETON_ROW_COUNT: usize = 10;

/// How many rows a ListView keeps widgets for around its anchor (GTK's
/// `GTK_LIST_VIEW_MAX_LIST_ITEMS`); rows outside that window render blank.
const GTK_MAX_ROW_WIDGETS: u32 = 200;

/// How far, in rows, the viewport can move before the list is re-anchored.
/// Must stay under GTK_MAX_ROW_WIDGETS / 2.
const REANCHOR_EVERY_ROWS: u32 = 50;

pub enum QueueMenuEntry {
    None,
    Add,
    Remove,
}

pub fn build_song_menu(
    song: &Track,
    show_view_album: bool,
    exclude_artist_id: Option<&str>,
    queue_entry: QueueMenuEntry,
    liked: Option<bool>,
) -> gio::MenuModel {
    let info_section = gio::Menu::new();
    if show_view_album {
        info_section.append(Some(&*labels::VIEW_ALBUM), Some("song.view_album"));
    }
    for artist in song
        .artists
        .iter()
        .filter(|a| exclude_artist_id != Some(a.rri.id.as_str()))
    {
        info_section.append(
            Some(&labels::more_from_label(&artist.name)),
            Some(&format!("song.view_artist_{}", artist.rri.id)),
        );
    }

    let queue_section = gio::Menu::new();
    match queue_entry {
        QueueMenuEntry::None => {}
        QueueMenuEntry::Add => {
            queue_section.append(Some(&*labels::ADD_TO_QUEUE), Some("song.queue"));
        }
        QueueMenuEntry::Remove => {
            queue_section.append(Some(&*labels::REMOVE_FROM_QUEUE), Some("song.dequeue"));
        }
    }
    if let Some(liked) = liked {
        let label = if liked {
            &*labels::UNLIKE
        } else {
            &*labels::LIKE
        };
        queue_section.append(Some(label), Some("song.like"));
    }

    let link_section = gio::Menu::new();
    link_section.append(Some(&*labels::COPY_LINK), Some("song.copy_link"));

    let menu = gio::Menu::new();
    if info_section.n_items() > 0 {
        menu.append_section(None, &info_section);
    }
    if queue_section.n_items() > 0 {
        menu.append_section(None, &queue_section);
    }
    menu.append_section(None, &link_section);
    menu.upcast()
}

pub trait TrackListModel: ProvidesApi {
    fn is_paused(&self) -> bool;

    fn song_list_model(&self) -> SongListModel;

    fn current_song_id(&self) -> Option<String>;

    fn play_song_at(&self, pos: usize, id: &str);

    fn autoscroll_to_playing(&self) -> bool {
        true
    }

    fn show_song_covers(&self) -> bool {
        true
    }

    fn show_disc_headers(&self) -> bool {
        false
    }

    fn show_album_column(&self) -> bool {
        false
    }

    fn show_loading_skeleton(&self) -> bool {
        true
    }

    fn actions_for(&self, _song: &Track) -> Option<SimpleActionGroup> {
        None
    }

    fn menu_for(&self, _song: &Track, _liked: bool) -> Option<gio::MenuModel> {
        None
    }

    fn select_song(&self, _id: &str) {}
    fn deselect_song(&self, _id: &str) {}
    fn enable_selection(&self) -> bool {
        false
    }

    fn selection(&self) -> Option<Box<dyn Deref<Target = SelectionState> + '_>> {
        None
    }

    fn is_selection_enabled(&self) -> bool {
        self.selection()
            .map(|s| s.is_selection_enabled())
            .unwrap_or(false)
    }

    fn toggle_select(&self, id: &str) {
        if let Some(selection) = self.selection() {
            if selection.is_song_selected(id) {
                self.deselect_song(id);
            } else {
                self.select_song(id);
            }
        }
    }

    fn is_song_liked(&self, _id: &str) -> bool {
        false
    }

    fn toggle_song_like(&self, _id: &str) {}

    fn skip_explicit(&self) -> bool {
        false
    }

    fn song_state(&self, id: &str) -> SongState {
        let is_explicit_filtered = self.skip_explicit()
            && self
                .song_list_model()
                .get(id)
                .is_some_and(|m| m.description().is_explicit());
        SongState {
            is_playing: self.current_song_id().is_some_and(|s| s == id),
            is_selected: self.selection().is_some_and(|s| s.is_song_selected(id)),
            is_liked: self.is_song_liked(id),
            is_explicit_filtered,
        }
    }

    fn load_more(&self) {}
}

fn row_at_viewport_center(list_top: f64, page_size: f64, n_rows: u32) -> Option<u32> {
    let last = n_rows.checked_sub(1)?;
    let row = ((page_size / 2.0 - list_top) / ROW_HEIGHT_PX as f64).max(0.0) as u32;
    Some(row.min(last))
}

/// The row to anchor the list on so its widget window is centered on row
/// `center`. GTK gives anchor `a` of `n_rows` `GTK_MAX_ROW_WIDGETS * a /
/// n_rows` widgets before it, so solve `a - that = center - window / 2`.
fn anchor_for_center(center: u32, n_rows: u32) -> u32 {
    let last = n_rows.saturating_sub(1);
    if n_rows <= GTK_MAX_ROW_WIDGETS {
        return center.min(last);
    }
    let (center, n, window) = (center as f64, n_rows as f64, GTK_MAX_ROW_WIDGETS as f64);
    let anchor = (center - window / 2.0) * n / (n - window);
    (anchor.max(0.0) as u32).min(last)
}

fn should_load_more(
    viewport_bottom: f64,
    content_height: f64,
    prefetch_margin: f64,
    loaded: usize,
    total: usize,
    is_complete: bool,
) -> bool {
    if is_complete || loaded >= total {
        return false;
    }
    viewport_bottom + prefetch_margin >= content_height
}

fn disc_header_text(disc: u32) -> String {
    // translators: Header shown above the tracks of disc {0} in a
    // multi-disc album.
    gettextrs::gettext!("Disc {}", disc)
}

/// Disc edges (rounded card corners) are part of the entry so a change in
/// them rebinds the row.
#[derive(Clone, PartialEq)]
enum ListEntry {
    DiscHeader(u32),
    Track {
        song: SongModel,
        disc_start: bool,
        disc_end: bool,
    },
    Placeholder {
        disc_start: bool,
        disc_end: bool,
    },
}

fn list_entry(item: &glib::Object) -> Option<ListEntry> {
    let wrapper = item.downcast_ref::<glib::BoxedAnyObject>()?;
    Some(wrapper.borrow::<ListEntry>().clone())
}

#[derive(Default)]
struct Projection {
    show_disc_headers: bool,
    show_skeleton: bool,
    positions: Vec<Option<usize>>,
    /// The wrapper object for each entry, keyed by (entry, occurrence), kept
    /// across rebuilds so unchanged rows keep their widgets.
    wrappers: HashMap<(String, usize), glib::BoxedAnyObject>,
}

impl Projection {
    /// Fills `store` with `songs`, plus disc headers and loading
    /// placeholders. Every position gets its own wrapper object: a track
    /// listed twice is the same SongModel twice, and ListView can't handle
    /// duplicate items (it warns, then may crash on a stale row widget).
    fn rebuild(&mut self, songs: &SongListModel, store: &gio::ListStore) {
        let tracks: Vec<(usize, SongModel)> = (0..songs.partial_len())
            .filter_map(|i| songs.index_continuous(i).map(|song| (i, song)))
            .collect();
        let discs: Vec<Option<u32>> = tracks
            .iter()
            .map(|(_, song)| song.description().disc_number)
            .collect();
        let multi_disc = self.show_disc_headers && discs.iter().any(|d| *d != discs[0]);
        let header_before =
            |n: usize| multi_disc && discs[n].is_some() && (n == 0 || discs[n] != discs[n - 1]);
        let last = tracks.len().saturating_sub(1);

        let mut entries = Vec::with_capacity(tracks.len());
        for (n, (i, song)) in tracks.into_iter().enumerate() {
            if header_before(n) {
                let disc = discs[n].unwrap_or_default();
                entries.push((format!("disc {disc}"), ListEntry::DiscHeader(disc), None));
            }
            let entry = ListEntry::Track {
                song: song.clone(),
                disc_start: n == 0 || header_before(n),
                disc_end: n == last || header_before(n + 1),
            };
            entries.push((song.get_id(), entry, Some(i)));
        }

        if self.show_skeleton && entries.is_empty() && !songs.is_complete() {
            for n in 0..SKELETON_ROW_COUNT {
                let entry = ListEntry::Placeholder {
                    disc_start: n == 0,
                    disc_end: n == SKELETON_ROW_COUNT - 1,
                };
                entries.push(("placeholder".to_string(), entry, None));
            }
        }

        let mut occurrences: HashMap<String, usize> = HashMap::new();
        let mut wrappers = HashMap::with_capacity(entries.len());
        let mut items: Vec<glib::Object> = Vec::with_capacity(entries.len());
        self.positions.clear();
        for (name, entry, position) in entries {
            let occurrence = occurrences.entry(name.clone()).or_default();
            let key = (name, *occurrence);
            *occurrence += 1;
            let wrapper = match self.wrappers.remove(&key) {
                Some(old) if *old.borrow::<ListEntry>() == entry => old,
                _ => glib::BoxedAnyObject::new(entry),
            };
            items.push(wrapper.clone().upcast());
            wrappers.insert(key, wrapper);
            self.positions.push(position);
        }
        self.wrappers = wrappers;
        store.splice(0, store.n_items(), &items);
    }
}

pub struct TrackList<Model> {
    animator: AnimatorDefault,
    listview: gtk::ListView,
    model: Rc<Model>,
    projection: Rc<RefCell<Projection>>,
}

impl<Model> TrackList<Model>
where
    Model: TrackListModel + 'static,
{
    pub fn new(listview: gtk::ListView, model: Rc<Model>) -> Self {
        let songs = model.song_list_model();

        let store = gio::ListStore::new::<glib::Object>();
        let projection = Rc::new(RefCell::new(Projection {
            show_disc_headers: model.show_disc_headers(),
            show_skeleton: model.show_loading_skeleton(),
            ..Default::default()
        }));
        projection.borrow_mut().rebuild(&songs, &store);
        // Synchronous, like SongListModel's own change notification: a
        // deferred rebuild could let the ListView see a torn model.
        songs.connect_items_changed(clone!(
            #[weak]
            store,
            #[weak]
            projection,
            move |songs, _, _, _| {
                projection.borrow_mut().rebuild(songs, &store);
            }
        ));

        let api_service = model.api_service();
        let options = RowOptions {
            show_cover: model.show_song_covers(),
            show_album: model.show_album_column(),
        };
        let factory = gtk::SignalListItemFactory::new();
        // Selection is the app's own (NoSelection); a selectable item makes
        // GTK select it on hover, which can crash on a stale row widget.
        factory.connect_setup(|_, item| {
            item.downcast_ref::<gtk::ListItem>()
                .unwrap()
                .set_selectable(false);
        });
        factory.connect_bind(clone!(
            #[weak]
            model,
            move |_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();
                match item.item().and_then(|i| list_entry(&i)) {
                    Some(ListEntry::Track {
                        song,
                        disc_start,
                        disc_end,
                    }) => bind_track(
                        item,
                        &song,
                        (disc_start, disc_end),
                        &model,
                        &api_service,
                        options,
                    ),
                    Some(ListEntry::DiscHeader(disc)) => bind_disc_header(item, disc),
                    Some(ListEntry::Placeholder {
                        disc_start,
                        disc_end,
                    }) => bind_placeholder(item, (disc_start, disc_end)),
                    None => {}
                }
            }
        ));
        factory.connect_unbind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            if let Some(ListEntry::Track { song, .. }) = item.item().and_then(|i| list_entry(&i)) {
                song.unbind_all();
            }
        });

        listview.add_css_class("playlist");
        listview.set_show_separators(false);
        listview.set_valign(gtk::Align::Start);
        listview.set_single_click_activate(true);
        listview.set_factory(Some(&factory));
        listview.set_model(Some(&gtk::NoSelection::new(Some(store.clone()))));

        listview.connect_activate(clone!(
            #[weak]
            songs,
            #[weak]
            model,
            #[weak]
            projection,
            move |_, position| {
                let Some(Some(index)) = projection
                    .borrow()
                    .positions
                    .get(position as usize)
                    .copied()
                else {
                    return;
                };
                let Some(song) = songs.index_continuous(index) else {
                    return;
                };
                let id = song.get_id();
                if model.is_selection_enabled() {
                    model.toggle_select(&id);
                } else {
                    model.play_song_at(index, &id);
                }
            }
        ));

        let long_press = gtk::GestureLongPress::new();
        long_press.set_touch_only(false);
        long_press.set_propagation_phase(gtk::PropagationPhase::Capture);
        long_press.connect_pressed(clone!(
            #[weak]
            model,
            move |_, _, _| {
                model.enable_selection();
            }
        ));
        listview.add_controller(long_press);

        let track_list = Self {
            animator: AnimatorDefault::ease_in_out_animator(),
            listview,
            model,
            projection,
        };
        track_list.update_paused();
        track_list.update_selection_mode();
        track_list.update_song_states(false);
        track_list
    }

    fn update_paused(&self) {
        set_css_class(&self.listview, "playlist--paused", self.model.is_paused());
    }

    fn update_selection_mode(&self) {
        set_css_class(
            &self.listview,
            "playlist--selectable",
            self.model.is_selection_enabled(),
        );
    }

    fn update_song_states(&self, autoscroll: bool) {
        let follow_playing =
            autoscroll && self.model.autoscroll_to_playing() && !self.model.is_selection_enabled();
        self.model.song_list_model().for_each(|i, song| {
            let state = self.model.song_state(&song.get_id());
            song.set_state(state);
            if state.is_playing && follow_playing {
                self.autoscroll_to_playing(i);
            }
        });
    }

    fn autoscroll_to_playing(&self, index: usize) {
        let Some(position) = self
            .projection
            .borrow()
            .positions
            .iter()
            .position(|entry| *entry == Some(index))
        else {
            return;
        };
        let Some(scrolled_window) = ancestor::<_, gtk::ScrolledWindow>(&self.listview) else {
            return;
        };
        let Some(content) = scrolled_content(&scrolled_window) else {
            return;
        };
        let adj = scrolled_window.vadjustment();
        let top = adj.value();
        let bottom = top + 0.9 * adj.page_size();
        let target = list_offset(&self.listview, &content) + position as f64 * ROW_HEIGHT_PX as f64;
        if target < top || target > bottom {
            self.animator.animate(
                20,
                clone!(
                    #[weak]
                    adj,
                    #[upgrade_or]
                    false,
                    move |p| {
                        let v = adj.value();
                        adj.set_value(v + p * (target - v));
                        true
                    }
                ),
            );
        }
    }

    /// Follows the page's ScrolledWindow, which scrolls this list along
    /// with the rest of the page:
    ///
    /// - Keeps the list's anchor on the visible rows. A ListView only has
    ///   widgets for GTK_MAX_ROW_WIDGETS rows around its anchor, and only
    ///   moves the anchor itself when it's the ScrolledWindow's direct
    ///   child, so without this rows past the first ~200 stay blank.
    /// - Calls `load_more()` when nearing the bottom.
    pub fn connect_scrolling(&self) {
        let Some(scrolled_window) = ancestor::<_, gtk::ScrolledWindow>(&self.listview) else {
            return;
        };
        let adj = scrolled_window.vadjustment();
        let model = Rc::clone(&self.model);
        let listview = self.listview.downgrade();
        let content = scrolled_content(&scrolled_window).map(|c| c.downgrade());
        // The viewport-center row the anchor was last set for.
        let anchored_center = Cell::new(0u32);
        let follow = Rc::new(move |adj: &gtk::Adjustment, force: bool| {
            let content = content.as_ref().and_then(|c| c.upgrade());
            let (Some(listview), Some(content)) = (listview.upgrade(), content) else {
                return;
            };
            let list_top = list_offset(&listview, &content) - adj.value();
            let n_rows = listview.model().map_or(0, |m| m.n_items());
            let Some(center) = row_at_viewport_center(list_top, adj.page_size(), n_rows) else {
                return;
            };
            if force || center.abs_diff(anchored_center.get()) >= REANCHOR_EVERY_ROWS {
                anchored_center.set(center);
                let anchor = anchor_for_center(center, n_rows);
                listview.scroll_to(anchor, gtk::ListScrollFlags::NONE, None);
            }
        });

        // GTK re-anchors on its own when focus moves to a row (click or
        // keyboard), so restore the centered anchor once it's done.
        let refollow = {
            let follow = Rc::clone(&follow);
            let adj = adj.clone();
            move || {
                let follow = Rc::clone(&follow);
                let adj = adj.clone();
                glib::idle_add_local_once(move || follow(&adj, true));
            }
        };
        let press = gtk::GestureClick::builder().button(0).build();
        press.connect_pressed({
            let refollow = refollow.clone();
            move |_, _, _, _| refollow()
        });
        self.listview.add_controller(press);
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(move |_, _, _, _| {
            refollow();
            glib::Propagation::Proceed
        });
        self.listview.add_controller(keys);

        let check = Rc::new(move |adj: &gtk::Adjustment| {
            follow(adj, false);

            let songs = model.song_list_model();
            // Start loading one page height before the bottom.
            let prefetch_margin = adj.page_size().max(1.0);
            if should_load_more(
                adj.value() + adj.page_size(),
                adj.upper(),
                prefetch_margin,
                songs.partial_len(),
                songs.len(),
                songs.is_complete(),
            ) {
                model.load_more();
            }
        });
        adj.connect_value_changed(clone!(
            #[strong]
            check,
            move |adj| check(adj)
        ));
        adj.connect_notify_local(
            Some("upper"),
            clone!(
                #[strong]
                check,
                move |adj, _| check(adj)
            ),
        );
        // The content may already be shorter than the viewport.
        check(&adj);
    }
}

fn scrolled_content(scrolled_window: &gtk::ScrolledWindow) -> Option<gtk::Widget> {
    scrolled_window.child().map(|child| {
        child
            .downcast_ref::<gtk::Viewport>()
            .and_then(|viewport| viewport.child())
            .unwrap_or(child)
    })
}

/// The list's top within the scrolled content. Unlike its position on
/// screen, this doesn't lag behind the adjustment inside value-changed.
fn list_offset(listview: &gtk::ListView, content: &gtk::Widget) -> f64 {
    listview
        .compute_point(content, &gtk::graphene::Point::zero())
        .map_or(0.0, |p| p.y() as f64)
}

fn item_row<W: IsA<gtk::Widget> + Default>(item: &gtk::ListItem) -> W {
    item.child().and_downcast::<W>().unwrap_or_else(|| {
        let row = W::default();
        item.set_child(Some(&row));
        row
    })
}

fn set_interactive(item: &gtk::ListItem, interactive: bool) {
    item.set_activatable(interactive);
    item.set_focusable(interactive);
}

fn bind_disc_header(item: &gtk::ListItem, disc: u32) {
    set_interactive(item, false);
    item_row::<DiscHeaderRow>(item).set_text(&disc_header_text(disc));
}

fn bind_placeholder(item: &gtk::ListItem, (disc_start, disc_end): (bool, bool)) {
    set_interactive(item, false);
    let row = item_row::<TrackRow>(item);
    row.bind_skeleton();
    row.set_disc_position(disc_start, disc_end);
}

fn bind_track<Model: TrackListModel + 'static>(
    item: &gtk::ListItem,
    song: &SongModel,
    (disc_start, disc_end): (bool, bool),
    model: &Rc<Model>,
    api_service: &Arc<ApiService>,
    options: RowOptions,
) {
    set_interactive(item, true);
    let row = item_row::<TrackRow>(item);
    row.bind(song, Arc::clone(api_service), options);
    row.set_disc_position(disc_start, disc_end);

    let track = song.description().clone();
    let actions = model.actions_for(&track).unwrap_or_default();
    let like = gio::SimpleAction::new("like", None);
    let id = track.rri.id.clone();
    let like_model = Rc::downgrade(model);
    like.connect_activate(move |_, _| {
        if let Some(model) = like_model.upgrade() {
            model.toggle_song_like(&id);
        }
    });
    actions.add_action(&like);
    row.set_actions(Some(actions.upcast_ref()));

    row.set_menu(model.menu_for(&track, song.get_liked()).as_ref());
    let model = Rc::clone(model);
    let handler = song.connect_notify_local(
        Some("liked"),
        clone!(
            #[weak]
            model,
            #[weak]
            row,
            move |song, _| {
                row.set_menu(model.menu_for(&track, song.get_liked()).as_ref());
            }
        ),
    );
    song.push_signal(handler);
}

impl SongModel {
    fn set_state(
        &self,
        SongState {
            is_playing,
            is_selected,
            is_liked,
            is_explicit_filtered,
        }: SongState,
    ) {
        self.set_playing(is_playing);
        self.set_selected(is_selected);
        self.set_liked(is_liked);
        self.set_explicit_filtered(is_explicit_filtered);
    }
}

impl<Model> EventListener for TrackList<Model>
where
    Model: TrackListModel + 'static,
{
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::PlaybackEvent(
                PlaybackEvent::TrackChanged(_) | PlaybackEvent::PlaybackStopped,
            ) => {
                self.update_paused();
                self.update_song_states(true);
            }
            AppEvent::PlaybackEvent(
                PlaybackEvent::PlaybackResumed | PlaybackEvent::PlaybackPaused,
            ) => {
                self.update_paused();
            }
            AppEvent::SelectionEvent(SelectionEvent::SelectionModeChanged(_)) => {
                self.update_selection_mode();
                self.update_song_states(true);
            }
            AppEvent::SelectionEvent(SelectionEvent::SelectionChanged)
            | AppEvent::BrowserEvent(BrowserEvent::SavedTracksUpdated) => {
                self.update_song_states(true);
            }
            // No autoscroll: these fire during pagination, and scrolling
            // would trigger more loading.
            AppEvent::PlaybackEvent(
                PlaybackEvent::PlaylistChanged | PlaybackEvent::SkipExplicitChanged(_),
            )
            | AppEvent::BrowserEvent(
                BrowserEvent::AlbumDetailsLoaded(_)
                | BrowserEvent::AlbumTracksAppended(_)
                | BrowserEvent::PlaylistDetailsLoaded(_)
                | BrowserEvent::PlaylistTracksAppended(_)
                | BrowserEvent::PlaylistTracksRemoved(_)
                | BrowserEvent::ArtistDetailsUpdated(_)
                | BrowserEvent::UserDetailsUpdated(_),
            ) => {
                self.update_song_states(false);
            }
            _ => {}
        }
    }
}

impl<Model> Component for TrackList<Model> {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.listview.upcast_ref()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::models::SongListModel;
    use crate::app::state::SelectionState;
    use std::cell::RefCell;

    struct MockTrackListModel {
        current_song: Option<String>,
        selection: RefCell<SelectionState>,
        selected: RefCell<Vec<String>>,
        deselected: RefCell<Vec<String>>,
    }

    impl MockTrackListModel {
        fn new(current_song: Option<&str>) -> Self {
            Self {
                current_song: current_song.map(String::from),
                selection: RefCell::new(SelectionState::default()),
                selected: RefCell::new(Vec::new()),
                deselected: RefCell::new(Vec::new()),
            }
        }
    }

    impl crate::app::ProvidesApi for MockTrackListModel {
        fn api_service(&self) -> std::sync::Arc<riff_api::ApiService> {
            unimplemented!("not exercised by these tests")
        }
    }

    impl TrackListModel for MockTrackListModel {
        fn is_paused(&self) -> bool {
            false
        }
        fn song_list_model(&self) -> SongListModel {
            SongListModel::new(50)
        }
        fn current_song_id(&self) -> Option<String> {
            self.current_song.clone()
        }
        fn play_song_at(&self, _pos: usize, _id: &str) {}
        fn select_song(&self, id: &str) {
            self.selected.borrow_mut().push(id.to_string());
        }
        fn deselect_song(&self, id: &str) {
            self.deselected.borrow_mut().push(id.to_string());
        }
        fn selection(&self) -> Option<Box<dyn Deref<Target = SelectionState> + '_>> {
            Some(Box::new(self.selection.borrow()))
        }
    }

    // Projection

    /// A fresh projection's positions after one rebuild.
    fn rebuild_projection(
        songs: &SongListModel,
        store: &gio::ListStore,
        show_disc_headers: bool,
    ) -> Vec<Option<usize>> {
        let mut projection = Projection {
            show_disc_headers,
            ..Default::default()
        };
        projection.rebuild(songs, store);
        projection.positions
    }

    fn entry_at(store: &gio::ListStore, i: u32) -> ListEntry {
        list_entry(&store.item(i).unwrap()).unwrap()
    }

    fn make_disc_track(id: &str, disc_number: Option<u32>) -> crate::app::models::Track {
        let mut track = crate::app::models::make_track(id);
        track.disc_number = disc_number;
        track
    }

    #[test]
    fn test_rebuild_projection_no_headers_when_disabled() {
        let mut list_model = SongListModel::new(50);
        list_model
            .append(vec![
                make_disc_track("a", Some(1)),
                make_disc_track("b", Some(2)),
            ])
            .commit();

        let store = gio::ListStore::new::<glib::Object>();
        let map = rebuild_projection(&list_model, &store, false);

        assert_eq!(store.n_items(), 2);
        assert_eq!(map, vec![Some(0), Some(1)]);
    }

    #[test]
    fn test_rebuild_projection_no_headers_when_single_disc() {
        let mut list_model = SongListModel::new(50);
        list_model
            .append(vec![
                make_disc_track("a", Some(1)),
                make_disc_track("b", Some(1)),
            ])
            .commit();

        let store = gio::ListStore::new::<glib::Object>();
        let map = rebuild_projection(&list_model, &store, true);

        assert_eq!(store.n_items(), 2);
        assert_eq!(map, vec![Some(0), Some(1)]);
    }

    #[test]
    fn test_rebuild_projection_no_headers_when_all_disc_numbers_unknown() {
        let mut list_model = SongListModel::new(50);
        list_model
            .append(vec![make_disc_track("a", None), make_disc_track("b", None)])
            .commit();

        let store = gio::ListStore::new::<glib::Object>();
        let map = rebuild_projection(&list_model, &store, true);

        assert_eq!(store.n_items(), 2);
        assert_eq!(map, vec![Some(0), Some(1)]);
    }

    #[test]
    fn test_rebuild_projection_inserts_header_at_each_disc_boundary() {
        let mut list_model = SongListModel::new(50);
        list_model
            .append(vec![
                make_disc_track("a", Some(1)),
                make_disc_track("b", Some(1)),
                make_disc_track("c", Some(2)),
                make_disc_track("d", Some(2)),
            ])
            .commit();

        let store = gio::ListStore::new::<glib::Object>();
        let map = rebuild_projection(&list_model, &store, true);

        // header, a, b, header, c, d
        assert_eq!(store.n_items(), 6);
        assert_eq!(map, vec![None, Some(0), Some(1), None, Some(2), Some(3)]);
        assert!(matches!(entry_at(&store, 1), ListEntry::Track { .. }));
        assert!(entry_at(&store, 0) == ListEntry::DiscHeader(1));
        assert!(entry_at(&store, 3) == ListEntry::DiscHeader(2));
    }

    #[test]
    fn test_rebuild_projection_position_map_survives_rebuild_after_append() {
        let mut list_model = SongListModel::new(50);
        list_model
            .append(vec![
                make_disc_track("a", Some(1)),
                make_disc_track("b", Some(2)),
            ])
            .commit();

        let store = gio::ListStore::new::<glib::Object>();
        let map1 = rebuild_projection(&list_model, &store, true);
        // header, a, header, b
        assert_eq!(map1, vec![None, Some(0), None, Some(1)]);

        list_model
            .append(vec![make_disc_track("c", Some(2))])
            .commit();
        let map2 = rebuild_projection(&list_model, &store, true);
        // header, a, header, b, c - the appended track shares disc 2 with
        // "b", so no new header for it.
        assert_eq!(map2, vec![None, Some(0), None, Some(1), Some(2)]);
        assert_eq!(store.n_items(), 5);
    }

    #[test]
    fn test_projection_repeated_track_gets_distinct_items() {
        let mut list_model = SongListModel::new(50);
        list_model
            .append(vec![
                crate::app::models::make_track("a"),
                crate::app::models::make_track("b"),
                crate::app::models::make_track("a"),
            ])
            .commit();

        let store = gio::ListStore::new::<glib::Object>();
        let map = rebuild_projection(&list_model, &store, false);

        assert_eq!(map, vec![Some(0), Some(1), Some(2)]);
        // Both "a"s are the same SongModel, but must be different items.
        let song = |i| match entry_at(&store, i) {
            ListEntry::Track { song, .. } => song,
            _ => panic!("expected a track"),
        };
        assert_eq!(song(0), song(2));
        assert_ne!(store.item(0).unwrap(), store.item(2).unwrap());
    }

    fn disc_edges(store: &gio::ListStore) -> Vec<Option<(bool, bool)>> {
        (0..store.n_items())
            .map(|i| match entry_at(store, i) {
                ListEntry::Track {
                    disc_start,
                    disc_end,
                    ..
                }
                | ListEntry::Placeholder {
                    disc_start,
                    disc_end,
                } => Some((disc_start, disc_end)),
                ListEntry::DiscHeader(_) => None,
            })
            .collect()
    }

    #[test]
    fn test_projection_marks_disc_edges() {
        let mut list_model = SongListModel::new(50);
        list_model
            .append(vec![
                make_disc_track("a", Some(1)),
                make_disc_track("b", Some(1)),
                make_disc_track("c", Some(2)),
            ])
            .commit();
        let store = gio::ListStore::new::<glib::Object>();
        rebuild_projection(&list_model, &store, true);

        // header, a (start), b (end), header, c (start and end)
        assert_eq!(
            disc_edges(&store),
            vec![
                None,
                Some((true, false)),
                Some((false, true)),
                None,
                Some((true, true))
            ]
        );
    }

    #[test]
    fn test_projection_new_last_track_updates_old_last_track() {
        // Loading a page after the list's last track: it's no longer the
        // end of its disc, so it must get a new item (and be rebound), while
        // unaffected rows keep theirs.
        let mut list_model = SongListModel::new(50);
        list_model
            .append(vec![
                crate::app::models::make_track("a"),
                crate::app::models::make_track("b"),
                crate::app::models::make_track("c"),
            ])
            .commit();
        let store = gio::ListStore::new::<glib::Object>();
        let mut projection = Projection::default();
        projection.rebuild(&list_model, &store);
        let before: Vec<glib::Object> = (0..3).map(|i| store.item(i).unwrap()).collect();
        assert_eq!(disc_edges(&store)[2], Some((false, true)));

        list_model
            .append(vec![crate::app::models::make_track("d")])
            .commit();
        projection.rebuild(&list_model, &store);

        assert_eq!(disc_edges(&store)[2], Some((false, false)));
        assert_eq!(disc_edges(&store)[3], Some((false, true)));
        assert_ne!(store.item(2).unwrap(), before[2]);
        assert_eq!(store.item(1).unwrap(), before[1]);
    }

    fn skeleton_projection() -> Projection {
        Projection {
            show_skeleton: true,
            ..Default::default()
        }
    }

    fn is_placeholder(store: &gio::ListStore, i: u32) -> bool {
        matches!(entry_at(store, i), ListEntry::Placeholder { .. })
    }

    #[test]
    fn test_projection_placeholders_while_loading() {
        let list_model = SongListModel::new(50); // empty, not complete
        let store = gio::ListStore::new::<glib::Object>();
        let mut projection = skeleton_projection();
        projection.rebuild(&list_model, &store);

        assert_eq!(store.n_items() as usize, SKELETON_ROW_COUNT);
        assert!((0..store.n_items()).all(|i| is_placeholder(&store, i)));
        // Not tracks: activating one does nothing.
        assert!(projection.positions.iter().all(Option::is_none));
        // One card: rounded at the first and last placeholder.
        let edges = disc_edges(&store);
        assert_eq!(edges[0], Some((true, false)));
        assert_eq!(edges[SKELETON_ROW_COUNT - 1], Some((false, true)));
    }

    #[test]
    fn test_projection_placeholders_replaced_by_tracks() {
        let mut list_model = SongListModel::new(50);
        let store = gio::ListStore::new::<glib::Object>();
        let mut projection = skeleton_projection();
        projection.rebuild(&list_model, &store);

        list_model
            .append(vec![crate::app::models::make_track("a")])
            .commit();
        projection.rebuild(&list_model, &store);

        assert_eq!(store.n_items(), 1);
        assert!(!is_placeholder(&store, 0));
    }

    #[test]
    fn test_projection_no_placeholders_when_empty_and_complete_or_disabled() {
        // Loaded, and empty (e.g. an empty playlist).
        let mut empty = SongListModel::new(50);
        empty.append(vec![]).commit();
        let store = gio::ListStore::new::<glib::Object>();
        skeleton_projection().rebuild(&empty, &store);
        assert_eq!(store.n_items(), 0);

        // Still loading, but the page turned placeholders off.
        let loading = SongListModel::new(50);
        Projection::default().rebuild(&loading, &store);
        assert_eq!(store.n_items(), 0);
    }

    #[test]
    fn test_projection_rebuild_reuses_unchanged_items() {
        let mut list_model = SongListModel::new(50);
        list_model
            .append(vec![
                crate::app::models::make_track("a"),
                crate::app::models::make_track("a"),
            ])
            .commit();
        let store = gio::ListStore::new::<glib::Object>();
        let mut projection = Projection::default();
        projection.rebuild(&list_model, &store);
        let before: Vec<glib::Object> = (0..2).map(|i| store.item(i).unwrap()).collect();

        list_model
            .append(vec![crate::app::models::make_track("c")])
            .commit();
        projection.rebuild(&list_model, &store);

        // The first row is unchanged, so it keeps its object (and the
        // ListView keeps its widget). The second was the last track and no
        // longer is, so it gets a new one.
        assert_eq!(store.n_items(), 3);
        assert_eq!(store.item(0).unwrap(), before[0]);
        assert_ne!(store.item(1).unwrap(), before[1]);
    }

    #[test]
    fn test_song_state_playing() {
        let model = MockTrackListModel::new(Some("song1"));
        let state = model.song_state("song1");
        assert!(state.is_playing);
        assert!(!state.is_selected);
    }

    #[test]
    fn test_song_state_not_playing() {
        let model = MockTrackListModel::new(Some("song1"));
        let state = model.song_state("song2");
        assert!(!state.is_playing);
    }

    #[test]
    fn test_song_state_no_current_song() {
        let model = MockTrackListModel::new(None);
        let state = model.song_state("song1");
        assert!(!state.is_playing);
    }

    #[test]
    fn test_is_selection_enabled_default_false() {
        let model = MockTrackListModel::new(None);
        assert!(!model.is_selection_enabled());
    }

    #[test]
    fn test_toggle_select_selects_unselected_song() {
        let model = MockTrackListModel::new(None);
        model.toggle_select("song1");
        assert_eq!(model.selected.borrow().as_slice(), &["song1"]);
        assert!(model.deselected.borrow().is_empty());
    }

    // row_at_viewport_center

    #[test]
    fn test_row_at_viewport_center_at_top() {
        // List starts 300px down a 600px viewport: the center (300px) is
        // the list's first row.
        assert_eq!(row_at_viewport_center(300.0, 600.0, 1000), Some(0));
    }

    #[test]
    fn test_row_at_viewport_center_scrolled() {
        // List top scrolled 56 * 700 px above the viewport; the center is
        // another 300px (5 rows) further.
        let top = -(ROW_HEIGHT_PX as f64 * 700.0);
        assert_eq!(row_at_viewport_center(top, 600.0, 1000), Some(705));
    }

    #[test]
    fn test_row_at_viewport_center_clamped_to_last_row() {
        assert_eq!(row_at_viewport_center(-1_000_000.0, 600.0, 10), Some(9));
    }

    #[test]
    fn test_row_at_viewport_center_empty_list() {
        assert_eq!(row_at_viewport_center(0.0, 600.0, 0), None);
    }

    // anchor_for_center: the window GTK builds for an anchor `a` of `n`
    // rows starts at a - 200 * a / n and spans 200 rows.

    fn window_start(anchor: u32, n: u32) -> f64 {
        anchor as f64 - GTK_MAX_ROW_WIDGETS as f64 * anchor as f64 / n as f64
    }

    #[test]
    fn test_anchor_for_center_centers_window_mid_list() {
        let anchor = anchor_for_center(500, 1000);
        assert!((window_start(anchor, 1000) - 400.0).abs() <= 1.0);
    }

    #[test]
    fn test_anchor_for_center_clamps_at_start() {
        assert_eq!(anchor_for_center(30, 1000), 0);
    }

    #[test]
    fn test_anchor_for_center_clamps_at_end() {
        let anchor = anchor_for_center(999, 1000);
        assert_eq!(anchor, 999);
        // The window still reaches ~100 rows above the last row.
        assert!(window_start(anchor, 1000) <= 899.0);
    }

    #[test]
    fn test_anchor_for_center_short_list() {
        assert_eq!(anchor_for_center(150, 180), 150);
        assert_eq!(anchor_for_center(500, 180), 179);
    }

    // should_load_more

    #[test]
    fn test_should_load_more_empty_list_at_top() {
        // Nothing loaded yet, viewport covers everything there is (0 height).
        assert!(should_load_more(0.0, 0.0, 50.0, 0, 100, false));
    }

    #[test]
    fn test_should_load_more_partial_window_far_from_bottom() {
        assert!(!should_load_more(200.0, 5000.0, 50.0, 50, 1000, false));
    }

    #[test]
    fn test_should_load_more_near_bottom_triggers() {
        assert!(should_load_more(950.0, 1000.0, 50.0, 50, 1000, false));
    }

    #[test]
    fn test_should_load_more_at_exact_bottom() {
        assert!(should_load_more(1000.0, 1000.0, 50.0, 50, 1000, false));
    }

    #[test]
    fn test_should_load_more_false_when_complete() {
        // Even at the bottom, a complete list must not request more.
        assert!(!should_load_more(1000.0, 1000.0, 50.0, 100, 100, true));
    }

    #[test]
    fn test_should_load_more_false_when_loaded_reaches_total() {
        assert!(!should_load_more(1000.0, 1000.0, 50.0, 1000, 1000, false));
    }

    #[test]
    fn test_should_load_more_guards_against_over_requesting() {
        // Loaded already exceeds the (possibly stale) total - must not loop.
        assert!(!should_load_more(1000.0, 1000.0, 50.0, 1200, 1000, false));
    }

    // disc_header_text

    #[test]
    fn test_disc_header_text_formats_number() {
        assert_eq!(disc_header_text(2), "Disc 2");
    }

    #[test]
    fn test_rebuild_projection_no_header_for_unknown_disc() {
        let mut list_model = SongListModel::new(50);
        list_model
            .append(vec![
                make_disc_track("a", Some(1)),
                make_disc_track("b", None),
                make_disc_track("c", Some(2)),
            ])
            .commit();

        let store = gio::ListStore::new::<glib::Object>();
        let map = rebuild_projection(&list_model, &store, true);

        // header, a, b, header, c - "b" starts a new (unknown) disc but has
        // no number to show.
        assert_eq!(map, vec![None, Some(0), Some(1), None, Some(2)]);
    }

    #[test]
    fn test_songs_stay_in_order_after_append() {
        use crate::app::models::make_track;

        let mut list_model = SongListModel::new(50);
        list_model
            .append(vec![make_track("a"), make_track("b"), make_track("c")])
            .commit();
        list_model
            .append(vec![make_track("d"), make_track("e")])
            .commit();

        let ids: Vec<String> = list_model.collect().into_iter().map(|s| s.rri.id).collect();
        assert_eq!(ids, vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn test_songs_accessible_after_partial_batch_then_full_batch() {
        use crate::app::models::{make_track, Page};

        let mut list_model = SongListModel::new(5);
        let batch1 = Page {
            items: vec![
                make_track("a"),
                make_track("b"),
                make_track("c"),
                make_track("d"),
                make_track("e"),
            ],
            offset: Some(0),
            total: None,
            next_cursor: None,
        };
        list_model.add(batch1).commit();
        assert_eq!(list_model.partial_len(), 5);

        let batch2 = Page {
            items: vec![
                make_track("f"),
                make_track("g"),
                make_track("h"),
                make_track("i"),
                make_track("j"),
            ],
            offset: Some(5),
            total: None,
            next_cursor: None,
        };
        list_model.add(batch2).commit();
        assert_eq!(list_model.partial_len(), 10);

        let ids: Vec<String> = list_model.collect().into_iter().map(|s| s.rri.id).collect();
        assert_eq!(ids, vec!["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]);
    }
}
