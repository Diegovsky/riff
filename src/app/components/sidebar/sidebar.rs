use gettextrs::gettext;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::{
    create_playlist::CreatePlaylistPopover,
    playlist_actions,
    sidebar_row::SidebarRow,
    SidebarDestination, SidebarItem, CREATE_PLAYLIST_ITEM, SAVED_PLAYLISTS_SECTION,
};
use crate::app::models::{AlbumModel, PlaylistSummary};
use crate::app::state::ScreenName;
use crate::app::{
    ActionDispatcher, AppAction, AppEvent, AppModel, BrowserAction, BrowserEvent, Component,
    EventListener,
};

const NUM_FIXED_ENTRIES: u32 = 6;
const NUM_PLAYLISTS: usize = 20;

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
            .take(NUM_PLAYLISTS)
            .map(Self::map_to_destination)
            .collect()
    }

    fn map_to_destination(a: AlbumModel) -> SidebarDestination {
        let title = Some(a.album())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| gettext("Unnamed playlist"));
        let id = a.uri();
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

    fn navigate(&self, dest: SidebarDestination) {
        let actions = match dest {
            SidebarDestination::Library
            | SidebarDestination::SavedTracks
            | SidebarDestination::NowPlaying
            | SidebarDestination::SavedPlaylists => {
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
}

impl Sidebar {
    pub fn new(listbox: gtk::ListBox, model: Rc<SidebarModel>) -> Self {
        let popover = CreatePlaylistPopover::new();
        popover.connect_create(clone!(
            #[weak]
            model,
            move |t| model.create_new_playlist(t)
        ));

        let list_store = gio::ListStore::new::<SidebarItem>();

        list_store.append(&SidebarItem::from_destination(SidebarDestination::Library));
        list_store.append(&SidebarItem::from_destination(
            SidebarDestination::SavedTracks,
        ));
        list_store.append(&SidebarItem::from_destination(
            SidebarDestination::NowPlaying,
        ));
        list_store.append(&SidebarItem::playlists_section());
        list_store.append(&SidebarItem::create_playlist_item());
        list_store.append(&SidebarItem::from_destination(
            SidebarDestination::SavedPlaylists,
        ));

        listbox.bind_model(
            Some(&list_store),
            clone!(
                #[weak]
                popover,
                #[upgrade_or_panic]
                move |obj| {
                    let item = obj.downcast_ref::<SidebarItem>().unwrap();
                    if item.navigatable() {
                        Self::make_navigatable(item)
                    } else {
                        match item.id().as_str() {
                            SAVED_PLAYLISTS_SECTION => Self::make_section_label(item),
                            CREATE_PLAYLIST_ITEM => Self::make_create_playlist(item, popover),
                            _ => unimplemented!(),
                        }
                    }
                }
            ),
        );

        listbox.connect_row_activated(clone!(
            #[weak]
            popover,
            #[weak]
            model,
            move |_, row| {
                if let Some(row) = row.downcast_ref::<SidebarRow>() {
                    if let Some(dest) = row.item().destination() {
                        model.navigate(dest);
                    } else {
                        match row.item().id().as_str() {
                            CREATE_PLAYLIST_ITEM => popover.popup(),
                            _ => unimplemented!(),
                        }
                    }
                }
            }
        ));

        let context_menu = gtk::PopoverMenu::from_model(None::<&gio::MenuModel>);
        context_menu.set_parent(&listbox);
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
                listbox.insert_action_group("playlist", Some(&actions));

                let is_owned = model.is_playlist_owned(&id);
                context_menu.set_menu_model(Some(&playlist_actions::build_playlist_menu(is_owned)));

                let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
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

        Self {
            listbox,
            list_store,
            model,
            _context_menu: context_menu,
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
            NUM_FIXED_ENTRIES,
            self.list_store.n_items() - NUM_FIXED_ENTRIES,
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
