use gdk::ffi::GDK_BUTTON_SECONDARY;
use gio::prelude::*;
use gio::SimpleActionGroup;
use gtk::{prelude::*, GestureClick};
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::labels;
use crate::app::components::utils::{ancestor, AnimatorDefault};
use crate::app::components::{Component, EventListener, SongWidget};
use crate::app::models::{SongListModel, SongModel, SongState, Track, TrackExt};
use crate::app::state::{BrowserEvent, PlaybackEvent, SelectionEvent, SelectionState};
use crate::app::{AppEvent, ProvidesApi};

/// Whether a song's context menu should offer a queue entry, and which one.
pub enum QueueMenuEntry {
    /// No queue entry (e.g. saved tracks, search results).
    None,
    /// "Add to Queue", appending the song to the play queue.
    Add,
    /// "Remove from Queue", used on the Now Playing page itself.
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

pub trait PlaylistModel: ProvidesApi {
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

    fn actions_for(&self, _song: &Track) -> Option<gio::ActionGroup> {
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

    fn is_song_liked(&self, _id: &str) -> bool {
        false
    }

    fn toggle_song_like(&self, _id: &str) {}

    fn skip_explicit(&self) -> bool {
        false
    }

    fn song_state(&self, id: &str) -> SongState {
        let is_playing = self.current_song_id().map(|s| s.eq(id)).unwrap_or(false);
        let is_selected = self
            .selection()
            .map(|s| s.is_song_selected(id))
            .unwrap_or(false);
        let is_liked = self.is_song_liked(id);
        let is_explicit_filtered = if self.skip_explicit() {
            self.song_list_model()
                .get(id)
                .map(|m| m.description().is_explicit())
                .unwrap_or(false)
        } else {
            false
        };
        SongState {
            is_selected,
            is_playing,
            is_liked,
            is_explicit_filtered,
        }
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
}

pub struct Playlist<Model> {
    animator: AnimatorDefault,
    listview: gtk::ListView,
    model: Rc<Model>,
}

impl<Model> Playlist<Model>
where
    Model: PlaylistModel + 'static,
{
    pub fn new(listview: gtk::ListView, model: Rc<Model>) -> Self {
        let list_model = model.song_list_model();
        let selection_model = gtk::NoSelection::new(Some(list_model.clone()));
        let factory = gtk::SignalListItemFactory::new();
        let api_service = model.api_service();

        listview.add_css_class("playlist");
        listview.set_show_separators(true);
        listview.set_valign(gtk::Align::Start);

        listview.set_factory(Some(&factory));
        listview.set_single_click_activate(true);
        listview.set_model(Some(&selection_model));
        Self::set_paused(&listview, model.is_paused());
        Self::set_selection_active(&listview, model.is_selection_enabled());

        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let widget = SongWidget::new();
            let control = GestureClick::builder()
                .button(GDK_BUTTON_SECONDARY as _)
                .build();
            control.connect_pressed(clone!(
                #[weak]
                widget,
                move |_, _, x, y| {
                    widget.show_menu(x, y);
                }
            ));
            widget.add_controller(control);
            item.set_child(Some(&widget));
        });

        factory.connect_bind(clone!(
            #[weak]
            model,
            #[strong]
            api_service,
            move |_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();
                let song_model = item.item().unwrap().downcast::<SongModel>().unwrap();

                let widget = item.child().unwrap().downcast::<SongWidget>().unwrap();
                widget.bind(&song_model, api_service.clone(), model.show_song_covers());

                let song = song_model.description();
                let actions = model.actions_for(&song);
                if let Some(group) = actions
                    .as_ref()
                    .and_then(|a| a.downcast_ref::<SimpleActionGroup>())
                {
                    let like = gio::SimpleAction::new("like", None);
                    let like_id = song.rri.id.clone();
                    like.connect_activate(clone!(
                        #[weak]
                        model,
                        move |_, _| {
                            model.toggle_song_like(&like_id);
                        }
                    ));
                    group.add_action(&like);
                }
                widget.set_actions(actions.as_ref());
                widget.set_menu(model.menu_for(&song, song_model.get_liked()).as_ref());

                let menu_song = song.clone();
                let handler_id = song_model.connect_notify_local(
                    Some("liked"),
                    clone!(
                        #[weak]
                        model,
                        #[weak]
                        widget,
                        move |song_model, _| {
                            widget.set_menu(
                                model.menu_for(&menu_song, song_model.get_liked()).as_ref(),
                            );
                        }
                    ),
                );
                song_model.push_signal(handler_id);

                let like_id = song.rri.id.clone();
                widget.connect_like(clone!(
                    #[weak]
                    model,
                    move || {
                        model.toggle_song_like(&like_id);
                    }
                ));
            }
        ));

        factory.connect_unbind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let song_model = item.item().unwrap().downcast::<SongModel>().unwrap();
            song_model.unbind_all();
            let widget = item.child().unwrap().downcast::<SongWidget>().unwrap();
            widget.disconnect_like();
        });

        listview.connect_activate(clone!(
            #[weak]
            list_model,
            #[weak]
            model,
            move |_, position| {
                let song = list_model
                    .index_continuous(position as usize)
                    .expect("attempt to access invalid index");
                let song = song.description();
                let selection_enabled = model.is_selection_enabled();
                if selection_enabled {
                    model.toggle_select(&song.rri.id);
                } else {
                    model.play_song_at(position as usize, &song.rri.id);
                }
            }
        ));

        let press_gesture = gtk::GestureLongPress::new();
        press_gesture.set_touch_only(false);
        press_gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
        press_gesture.connect_pressed(clone!(
            #[weak]
            model,
            move |_, _, _| {
                model.enable_selection();
            }
        ));
        listview.add_controller(press_gesture);

        let playlist = Self {
            animator: AnimatorDefault::ease_in_out_animator(),
            listview,
            model,
        };

        // Seed the state (playing/selected/liked) of any items already present
        // in the model. This runs during component construction, which happens
        // outside AppState's borrow_mut, so reading AppState here is safe. It is
        // required because the bind callback no longer pulls state from AppState;
        // without this, pre-existing rows (e.g. the current track in the queue
        // when opening the Now Playing page) would render with default state.
        // Note: no autoscroll here — construction should not move the viewport.
        playlist.seed_song_states();

        playlist
    }

    fn autoscroll_to_playing(&self, index: usize) {
        let len = self.model.song_list_model().partial_len() as f64;
        let scrolled_window: Option<gtk::ScrolledWindow> = ancestor(&self.listview);
        let adj = scrolled_window.map(|w| w.vadjustment());
        if let Some(adj) = adj {
            let v = adj.value();
            let v2 = v + 0.9 * adj.page_size();
            let pos = (index as f64) * adj.upper() / len;
            debug!("estimated pos: {}", pos);
            debug!("current window: {} -- {}", v, v2);
            if pos < v || pos > v2 {
                self.animator.animate(
                    20,
                    clone!(
                        #[weak]
                        adj,
                        #[upgrade_or]
                        false,
                        move |p| {
                            let v = adj.value();
                            adj.set_value(v + p * (pos - v));
                            true
                        }
                    ),
                );
            }
        }
    }

    fn seed_song_states(&self) {
        self.model.song_list_model().for_each(|_, model_song| {
            let state = self.model.song_state(&model_song.get_id());
            model_song.set_state(state);
        });
    }

    /// Like `seed_song_states`, but additionally autoscrolls to the currently
    /// playing track. Only appropriate for events where following the playing
    /// track is the intended behavior (e.g. TrackChanged).
    fn update_list(&self) {
        let autoscroll_to_playing = self.model.autoscroll_to_playing();
        let is_selection_enabled = self.model.is_selection_enabled();

        self.model.song_list_model().for_each(|i, model_song| {
            let state = self.model.song_state(&model_song.get_id());
            model_song.set_state(state);
            if state.is_playing && autoscroll_to_playing && !is_selection_enabled {
                self.autoscroll_to_playing(i);
            }
        });
    }

    fn set_selection_active(listview: &gtk::ListView, active: bool) {
        let class_name = "playlist--selectable";
        if active {
            listview.add_css_class(class_name);
        } else {
            listview.remove_css_class(class_name);
        }
    }

    fn set_paused(listview: &gtk::ListView, paused: bool) {
        let class_name = "playlist--paused";
        if paused {
            listview.add_css_class(class_name);
        } else {
            listview.remove_css_class(class_name);
        }
    }
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

impl<Model> EventListener for Playlist<Model>
where
    Model: PlaylistModel + 'static,
{
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::SelectionEvent(SelectionEvent::SelectionChanged) => {
                self.update_list();
            }
            AppEvent::PlaybackEvent(PlaybackEvent::TrackChanged(_)) => {
                Self::set_paused(&self.listview, self.model.is_paused());
                self.update_list();
            }
            AppEvent::PlaybackEvent(
                PlaybackEvent::PlaybackResumed | PlaybackEvent::PlaybackPaused,
            ) => {
                Self::set_paused(&self.listview, self.model.is_paused());
            }
            AppEvent::SelectionEvent(SelectionEvent::SelectionModeChanged(_)) => {
                Self::set_selection_active(&self.listview, self.model.is_selection_enabled());
                self.update_list();
            }
            AppEvent::BrowserEvent(BrowserEvent::SavedTracksUpdated) => {
                self.update_list();
            }
            // Content-change events: the backing song list gained or lost items
            // (initial load, pagination, queue changes, removals). Re-seed each
            // SongModel's state because the bind callback no longer pulls it from
            // AppState. This runs post-dispatch (borrow released), so reading
            // AppState is safe. Crucially we seed WITHOUT autoscrolling — these
            // events fire during pagination, and autoscrolling here would drive a
            // scroll -> load_more -> content-event feedback loop.
            AppEvent::PlaybackEvent(PlaybackEvent::PlaylistChanged) => {
                self.seed_song_states();
            }
            AppEvent::BrowserEvent(
                BrowserEvent::AlbumDetailsLoaded(_)
                | BrowserEvent::AlbumTracksAppended(_)
                | BrowserEvent::PlaylistDetailsLoaded(_)
                | BrowserEvent::PlaylistTracksAppended(_)
                | BrowserEvent::PlaylistTracksRemoved(_)
                | BrowserEvent::ArtistDetailsUpdated(_)
                | BrowserEvent::UserDetailsUpdated(_),
            ) => {
                self.seed_song_states();
            }
            // Re-seed so songs pick up the new explicit-filtered state.
            AppEvent::PlaybackEvent(PlaybackEvent::SkipExplicitChanged(_)) => {
                self.seed_song_states();
            }
            _ => {}
        }
    }
}

impl<Model> Component for Playlist<Model> {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.listview.upcast_ref()
    }
}
