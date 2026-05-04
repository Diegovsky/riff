use gdk::prelude::*;
use gio::SimpleActionGroup;
use std::rc::Rc;

use super::SidebarModel;
use crate::app::components::labels;

fn make_copy_link_action(id: &str) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("copy_link", None);
    let id = id.to_owned();
    action.connect_activate(move |_, _| {
        let link = format!("https://open.spotify.com/playlist/{id}");
        let clipboard = gdk::Display::default().unwrap().clipboard();
        clipboard
            .set_content(Some(&gdk::ContentProvider::for_value(&link.to_value())))
            .expect("Failed to set clipboard content");
    });
    action
}

fn make_unfollow_action(id: &str, model: &Rc<SidebarModel>) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("unfollow", None);
    let id = id.to_owned();
    action.connect_activate(clone!(
        #[weak]
        model,
        move |_, _| {
            model.unfollow_playlist(id.clone());
        }
    ));
    action
}

pub fn build_playlist_actions(id: &str, model: &Rc<SidebarModel>) -> SimpleActionGroup {
    let group = SimpleActionGroup::new();
    group.add_action(&make_copy_link_action(id));
    group.add_action(&make_unfollow_action(id, model));
    group
}

pub fn build_playlist_menu(is_owned: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some(&*labels::COPY_LINK), Some("playlist.copy_link"));
    if is_owned {
        menu.append(Some(&*labels::DELETE_PLAYLIST), Some("playlist.unfollow"));
    } else {
        menu.append(Some(&*labels::UNFOLLOW_PLAYLIST), Some("playlist.unfollow"));
    }
    menu
}
