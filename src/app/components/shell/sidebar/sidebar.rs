use gettextrs::gettext;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::{
    create_playlist::CreatePlaylistPopover, playlist_actions, sidebar_row::SidebarRow,
    SidebarDestination, SidebarItem, CREATE_PLAYLIST_ITEM, LIBRARY_SECTION,
    SAVED_PLAYLISTS_SECTION,
};
use crate::app::models::{CardModel, PlaylistSummary};
use crate::app::state::{PlaybackAction, ScreenName};
use crate::app::{
    ActionDispatcher, AppAction, AppEvent, AppModel, BrowserAction, BrowserEvent, Component,
    EventListener, PaginationTarget, SongsSource,
};
use crate::feature_flags::{self, FeatureFlag};

pub struct SidebarModel {
    app_model: Rc<AppModel>,
    dispatcher: Box<dyn ActionDispatcher>,
}

impl SidebarModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self {
            app_model,
            dispatcher,
        }
    }

    fn get_playlists(&self) -> Vec<SidebarDestination> {
        self.app_model
            .get_state()
            .browser
            .home_state()
            .expect("expected HomeState to be available")
            .playlists
            .iter()
            .map(Self::map_to_destination)
            .collect()
    }

    pub fn load_more_playlists(&self) -> Option<()> {
        let api = self.app_model.get_spotify();
        let state = self.app_model.get_state();
        let home = state.browser.home_state()?;
        let batch_size = home.next_playlists_page.batch_size;
        let offset = home.next_playlists_page.next_offset?;
        drop(state);

        self.app_model
            .update_state(BrowserAction::ConsumeNextPage(PaginationTarget::SavedPlaylists).into());

        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                api.get_saved_playlists(offset, batch_size)
                    .await
                    .map(|playlists| BrowserAction::AppendPlaylistsContent(playlists).into())
            });

        Some(())
    }

    fn map_to_destination(a: CardModel) -> SidebarDestination {
        let title = Some(a.title())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| gettext("Unnamed playlist"));
        let id = a.id();
        SidebarDestination::Playlist(PlaylistSummary { id, title })
    }

    fn create_new_playlist(&self, name: String) {
        let user_id = self.app_model.get_state().logged_user.user.clone().unwrap();
        let api = self.app_model.get_spotify();
        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                api.create_new_playlist(name.as_str(), user_id.as_str())
                    .await
                    .map(AppAction::CreatePlaylist)
            })
    }

    pub(super) fn is_playlist_owned(&self, id: &str) -> bool {
        self.app_model
            .get_state()
            .logged_user
            .playlist_ids
            .contains(id)
    }

    pub(super) fn unfollow_playlist(&self, id: String) {
        let api = self.app_model.get_spotify();
        self.dispatcher
            .call_spotify_and_dispatch(move || async move {
                api.unfollow_playlist(&id).await?;
                Ok(AppAction::RemovePlaylist(id))
            })
    }

    pub(super) fn play_playlist(&self, id: String) {
        let api = self.app_model.get_spotify();
        let source = SongsSource::Playlist(id.clone());
        self.dispatcher
            .call_spotify_and_dispatch_many(move || async move {
                let batch = api.get_playlist_tracks(&id, 0, 100).await?;
                let first_id = batch.songs.first().map(|s| s.id.clone());
                let mut actions: Vec<AppAction> = vec![
                    PlaybackAction::SetShuffled(false).into(),
                    PlaybackAction::LoadPagedSongs(source, batch).into(),
                ];
                if let Some(track_id) = first_id {
                    actions.push(PlaybackAction::Load(track_id).into());
                }
                Ok(actions)
            });
    }

    pub(super) fn shuffle_playlist(&self, id: String) {
        let api = self.app_model.get_spotify();
        let source = SongsSource::Playlist(id.clone());
        self.dispatcher
            .call_spotify_and_dispatch_many(move || async move {
                let batch = api.get_playlist_tracks(&id, 0, 100).await?;
                let len = batch.songs.len();
                let track_id = if len > 0 {
                    let index = rand::random::<usize>() % len;
                    Some(batch.songs[index].id.clone())
                } else {
                    None
                };
                let mut actions: Vec<AppAction> = vec![
                    PlaybackAction::SetShuffled(true).into(),
                    PlaybackAction::LoadPagedSongs(source, batch).into(),
                ];
                if let Some(track_id) = track_id {
                    actions.push(PlaybackAction::Load(track_id).into());
                }
                Ok(actions)
            });
    }

    fn navigate(&self, dest: SidebarDestination) {
        let actions = match dest {
            SidebarDestination::Library
            | SidebarDestination::SavedTracks
            | SidebarDestination::NowPlaying
            | SidebarDestination::SavedPlaylists
            | SidebarDestination::SavedArtists => {
                vec![
                    BrowserAction::NavigationPopTo(ScreenName::Home).into(),
                    BrowserAction::SetHomeVisiblePage(dest.id()).into(),
                ]
            }
            SidebarDestination::Playlist(PlaylistSummary { id, .. }) => {
                vec![AppAction::ViewPlaylist(id)]
            }
        };
        self.dispatcher.dispatch_many(actions);
    }
}

pub struct Sidebar {
    listbox: gtk::ListBox,
    list_store: gio::ListStore,
    model: Rc<SidebarModel>,
    _context_menu: gtk::PopoverMenu,
    num_fixed_entries: u32,
}

impl Sidebar {
    pub fn new(listbox: gtk::ListBox, model: Rc<SidebarModel>) -> Self {
        let create_playlist_enabled = feature_flags::is_enabled(FeatureFlag::CreateNewPlaylist);

        let popover = if create_playlist_enabled {
            let p = CreatePlaylistPopover::new();
            p.connect_create(clone!(
                #[weak]
                model,
                move |t| model.create_new_playlist(t)
            ));
            Some(p)
        } else {
            None
        };

        let list_store = gio::ListStore::new::<SidebarItem>();

        list_store.append(&SidebarItem::from_destination(
            SidebarDestination::NowPlaying,
        ));
        list_store.append(&SidebarItem::from_destination(
            SidebarDestination::SavedArtists,
        ));
        list_store.append(&SidebarItem::from_destination(SidebarDestination::Library));
        list_store.append(&SidebarItem::from_destination(
            SidebarDestination::SavedPlaylists,
        ));
        list_store.append(&SidebarItem::from_destination(
            SidebarDestination::SavedTracks,
        ));
        list_store.append(&SidebarItem::playlists_section());
        if create_playlist_enabled {
            list_store.append(&SidebarItem::create_playlist_item());
        }

        listbox.bind_model(
            Some(&list_store),
            clone!(
                #[strong]
                popover,
                move |obj| {
                    let item = obj.downcast_ref::<SidebarItem>().unwrap();
                    if item.navigatable() {
                        Self::make_navigatable(item)
                    } else {
                        match item.id().as_str() {
                            SAVED_PLAYLISTS_SECTION | LIBRARY_SECTION => {
                                Self::make_section_label(item)
                            }
                            CREATE_PLAYLIST_ITEM => Self::make_create_playlist(
                                item,
                                popover.clone().expect("popover should exist"),
                            ),
                            _ => unimplemented!(),
                        }
                    }
                }
            ),
        );

        listbox.connect_row_activated(clone!(
            #[strong]
            popover,
            #[weak]
            model,
            move |_, row| {
                if let Some(row) = row.downcast_ref::<SidebarRow>() {
                    if let Some(dest) = row.item().destination() {
                        model.navigate(dest);
                    } else {
                        match row.item().id().as_str() {
                            CREATE_PLAYLIST_ITEM => {
                                if let Some(ref popover) = popover {
                                    popover.popup();
                                }
                            }
                            _ => unimplemented!(),
                        }
                    }
                }
            }
        ));

        let context_menu = gtk::PopoverMenu::from_model(None::<&gio::MenuModel>);
        // Parent the popover to the Box above the ScrolledWindow to avoid
        // inheriting any scroll constraints that would add a scrollbar.
        let sidebar_box = listbox
            .ancestor(gtk::ScrolledWindow::static_type())
            .and_then(|sw| sw.parent())
            .and_downcast::<gtk::Box>()
            .unwrap();
        context_menu.set_parent(&sidebar_box);
        context_menu.set_has_arrow(false);

        let context_row: Rc<RefCell<Option<SidebarRow>>> = Default::default();

        context_menu.connect_closed(clone!(
            #[strong]
            context_row,
            move |_| {
                if let Some(row) = context_row.borrow_mut().take() {
                    row.unset_state_flags(gtk::StateFlags::SELECTED);
                }
            }
        ));

        let show_context_menu = clone!(
            #[weak]
            listbox,
            #[weak]
            model,
            #[weak]
            context_menu,
            #[strong]
            context_row,
            move |x: f64, y: f64| {
                let Some(row) = listbox.row_at_y(y as i32) else {
                    return;
                };
                let Some(row) = row.downcast_ref::<SidebarRow>() else {
                    return;
                };
                let Some(SidebarDestination::Playlist(PlaylistSummary { id, .. })) =
                    row.item().destination()
                else {
                    return;
                };

                row.set_state_flags(gtk::StateFlags::SELECTED, false);
                context_row.replace(Some(row.clone()));

                let actions = playlist_actions::build_playlist_actions(&id, &model);
                context_menu.insert_action_group("playlist", Some(&actions));

                let is_owned = model.is_playlist_owned(&id);
                context_menu.set_menu_model(Some(&playlist_actions::build_playlist_menu(is_owned)));

                // Translate coordinates from listbox space to the popover parent (sidebar Box) space
                let popover_parent = context_menu.parent().unwrap();
                let translated = listbox
                    .compute_point(
                        &popover_parent,
                        &gtk::graphene::Point::new(x as f32, y as f32),
                    )
                    .unwrap_or_else(|| gtk::graphene::Point::new(x as f32, y as f32));
                let rect = gdk::Rectangle::new(translated.x() as i32, translated.y() as i32, 1, 1);
                context_menu.set_pointing_to(Some(&rect));
                context_menu.popup();
            }
        );

        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        right_click.connect_pressed(clone!(
            #[strong]
            show_context_menu,
            move |_, _, x, y| {
                show_context_menu(x, y);
            }
        ));
        listbox.add_controller(right_click);

        let long_press = gtk::GestureLongPress::new();
        long_press.set_touch_only(false);
        long_press.connect_pressed(clone!(
            #[strong]
            show_context_menu,
            move |_, x, y| {
                show_context_menu(x, y);
            }
        ));
        listbox.add_controller(long_press);

        let scrolled_window = listbox
            .ancestor(gtk::ScrolledWindow::static_type())
            .and_downcast::<gtk::ScrolledWindow>()
            .unwrap();
        scrolled_window.connect_edge_reached(clone!(
            #[weak]
            model,
            move |_, pos| {
                if pos == gtk::PositionType::Bottom {
                    model.load_more_playlists();
                }
            }
        ));

        let num_fixed_entries = list_store.n_items();

        Self {
            listbox,
            list_store,
            model,
            _context_menu: context_menu,
            num_fixed_entries,
        }
    }

    fn make_navigatable(item: &SidebarItem) -> gtk::Widget {
        let row = SidebarRow::new(item.clone());
        row.set_selectable(false);
        row.upcast()
    }

    fn make_section_label(item: &SidebarItem) -> gtk::Widget {
        let label = gtk::Label::new(Some(item.title().as_str()));
        label.add_css_class("caption-heading");
        let row = gtk::ListBoxRow::builder()
            .activatable(false)
            .selectable(false)
            .sensitive(false)
            .child(&label)
            .build();
        row.upcast()
    }

    fn make_create_playlist(item: &SidebarItem, popover: CreatePlaylistPopover) -> gtk::Widget {
        let row = SidebarRow::new(item.clone());
        row.set_activatable(true);
        row.set_selectable(false);
        row.set_sensitive(true);
        popover.set_parent(&row);
        row.upcast()
    }

    fn update_playlists_in_sidebar(&self) {
        let playlists: Vec<SidebarItem> = self
            .model
            .get_playlists()
            .into_iter()
            .map(SidebarItem::from_destination)
            .collect();
        self.list_store.splice(
            self.num_fixed_entries,
            self.list_store.n_items() - self.num_fixed_entries,
            playlists.as_slice(),
        );
    }
}

impl Component for Sidebar {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.listbox.upcast_ref()
    }
}

impl EventListener for Sidebar {
    fn on_event(&mut self, event: &AppEvent) {
        if let AppEvent::BrowserEvent(BrowserEvent::SavedPlaylistsUpdated) = event {
            self.update_playlists_in_sidebar();
        }
    }
}
