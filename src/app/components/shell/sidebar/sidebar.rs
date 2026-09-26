use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::{
    context_menu::build_context_menu, create_playlist::CreatePlaylistPopover,
    sidebar_row::SidebarRow, SidebarDestination, SidebarItem, SidebarModel, CREATE_PLAYLIST_ITEM,
    LIBRARY_SECTION, PINNED_SECTION, SAVED_PLAYLISTS_SECTION,
};
use crate::app::{AppEvent, BrowserEvent, Component, EventListener};
use crate::feature_flags::{is_enabled, FeatureFlag};

pub struct Sidebar {
    listbox: gtk::ListBox,
    list_store: gio::ListStore,
    model: Rc<SidebarModel>,
    _context_menu: gtk::PopoverMenu,
    num_fixed_entries: u32,
}

impl Sidebar {
    pub fn new(listbox: gtk::ListBox, model: Rc<SidebarModel>) -> Self {
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

        model.apply_sidebar_items(&list_store, num_fixed_entries);

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
        self.model
            .apply_sidebar_items(&self.list_store, self.num_fixed_entries);
    }
}

impl Component for Sidebar {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.listbox.upcast_ref()
    }
}

impl EventListener for Sidebar {
    fn on_event(&mut self, event: &AppEvent) {
        if matches!(
            event,
            AppEvent::BrowserEvent(BrowserEvent::SavedPlaylistsUpdated)
                | AppEvent::BrowserEvent(BrowserEvent::PinnedPlaylistsUpdated)
        ) {
            self.update_playlists_in_sidebar();
        }
    }
}
