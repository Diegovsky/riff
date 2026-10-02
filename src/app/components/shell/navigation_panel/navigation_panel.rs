use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::{
    context_menu::build_context_menu, create_playlist::CreatePlaylistPopover,
    navigation_panel_row::NavigationPanelRow, NavigationPanelDestination, NavigationPanelItem,
    NavigationPanelModel, CREATE_PLAYLIST_ITEM, LIBRARY_SECTION, PINNED_SECTION,
    SAVED_PLAYLISTS_SECTION,
};
use crate::app::state::PlaybackEvent;
use crate::app::{AppEvent, BrowserEvent, Component, EventListener};
use crate::feature_flags::{is_enabled, FeatureFlag};

pub struct NavigationPanel {
    listbox: gtk::ListBox,
    list_store: gio::ListStore,
    model: Rc<NavigationPanelModel>,
    _context_menu: gtk::PopoverMenu,
    num_fixed_entries: u32,
}

impl NavigationPanel {
    pub fn new(listbox: gtk::ListBox, model: Rc<NavigationPanelModel>) -> Self {
        let create_playlist_enabled = is_enabled(FeatureFlag::CreateNewPlaylist);

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

        let list_store = gio::ListStore::new::<NavigationPanelItem>();

        list_store.append(&NavigationPanelItem::from_destination(
            NavigationPanelDestination::NowPlaying,
        ));
        list_store.append(&NavigationPanelItem::from_destination(
            NavigationPanelDestination::SavedArtists,
        ));
        list_store.append(&NavigationPanelItem::from_destination(
            NavigationPanelDestination::Library,
        ));
        list_store.append(&NavigationPanelItem::from_destination(
            NavigationPanelDestination::SavedPlaylists,
        ));
        list_store.append(&NavigationPanelItem::from_destination(
            NavigationPanelDestination::SavedTracks,
        ));
        list_store.append(&NavigationPanelItem::playlists_section());
        if create_playlist_enabled {
            list_store.append(&NavigationPanelItem::create_playlist_item());
        }

        listbox.bind_model(
            Some(&list_store),
            clone!(
                #[strong]
                popover,
                move |obj| {
                    let item = obj.downcast_ref::<NavigationPanelItem>().unwrap();
                    if item.navigatable() {
                        Self::make_navigatable(item)
                    } else {
                        match item.id().as_str() {
                            SAVED_PLAYLISTS_SECTION | PINNED_SECTION | LIBRARY_SECTION => {
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
                if let Some(row) = row.downcast_ref::<NavigationPanelRow>() {
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
        let navigation_panel_box = listbox
            .ancestor(gtk::ScrolledWindow::static_type())
            .and_then(|sw| sw.parent())
            .and_downcast::<gtk::Box>()
            .unwrap();
        context_menu.set_parent(&navigation_panel_box);
        context_menu.set_has_arrow(false);

        let context_row: Rc<RefCell<Option<NavigationPanelRow>>> = Default::default();

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
                let Some(row) = row.downcast_ref::<NavigationPanelRow>() else {
                    return;
                };
                let Some((prefix, actions, menu)) = row
                    .item()
                    .destination()
                    .and_then(|destination| build_context_menu(&destination, &model))
                else {
                    return;
                };

                row.set_state_flags(gtk::StateFlags::SELECTED, false);
                context_row.replace(Some(row.clone()));

                context_menu.insert_action_group(prefix, Some(&actions));
                context_menu.set_menu_model(Some(&menu));

                // Translate coordinates from listbox space to the popover parent (navigation panel Box) space
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

        model.apply_navigation_panel_items(&list_store, num_fixed_entries);

        let panel = Self {
            listbox,
            list_store,
            model,
            _context_menu: context_menu,
            num_fixed_entries,
        };
        panel.update_playing();
        panel
    }

    fn make_navigatable(item: &NavigationPanelItem) -> gtk::Widget {
        let row = NavigationPanelRow::new(item.clone());
        row.set_selectable(false);
        row.upcast()
    }

    fn make_section_label(item: &NavigationPanelItem) -> gtk::Widget {
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

    fn make_create_playlist(
        item: &NavigationPanelItem,
        popover: CreatePlaylistPopover,
    ) -> gtk::Widget {
        let row = NavigationPanelRow::new(item.clone());
        row.set_activatable(true);
        row.set_selectable(false);
        row.set_sensitive(true);
        popover.set_parent(&row);
        row.upcast()
    }

    // The Now Playing row's icon (see app.css)
    fn update_playing(&self) {
        if self.model.is_playing() {
            self.listbox.remove_css_class("now-playing--idle");
        } else {
            self.listbox.add_css_class("now-playing--idle");
        }
    }

    fn update_playlists_in_navigation_panel(&self) {
        self.model
            .apply_navigation_panel_items(&self.list_store, self.num_fixed_entries);
    }
}

impl Component for NavigationPanel {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.listbox.upcast_ref()
    }
}

impl EventListener for NavigationPanel {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::BrowserEvent(
                BrowserEvent::SavedPlaylistsUpdated | BrowserEvent::PinnedPlaylistsUpdated,
            ) => self.update_playlists_in_navigation_panel(),
            AppEvent::PlaybackEvent(
                PlaybackEvent::PlaybackPaused
                | PlaybackEvent::PlaybackResumed
                | PlaybackEvent::PlaybackStopped
                | PlaybackEvent::TrackChanged(_),
            ) => self.update_playing(),
            _ => {}
        }
    }
}
