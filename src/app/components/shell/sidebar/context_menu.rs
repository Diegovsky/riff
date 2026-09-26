use gdk::prelude::*;
use gio::SimpleActionGroup;
use std::rc::Rc;

use super::{SidebarDestination, SidebarModel};
use crate::app::components::labels;
use crate::app::models::PlaylistSummary;
use crate::feature_flags::{is_enabled, FeatureFlag};
use crate::settings;

fn make_play_action(id: &str, model: &Rc<SidebarModel>) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("play", None);
    let id = id.to_owned();
    action.connect_activate(clone!(
        #[weak]
        model,
        move |_, _| {
            model.play_playlist(id.clone());
        }
    ));
    action
}

fn make_shuffle_action(id: &str, model: &Rc<SidebarModel>) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("shuffle", None);
    let id = id.to_owned();
    action.connect_activate(clone!(
        #[weak]
        model,
        move |_, _| {
            model.shuffle_playlist(id.clone());
        }
    ));
    action
}

fn make_copy_link_action(id: &str) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("copy_link", None);
    let id = id.to_owned();
    action.connect_activate(move |_, _| {
        let link = format!("https://open.spotify.com/playlist/{id}");
        crate::app::components::copy_link_to_clipboard(&link);
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

fn make_toggle_pin_action(id: &str, model: &Rc<SidebarModel>) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("toggle_pin", None);
    let id = id.to_owned();
    action.connect_activate(clone!(
        #[weak]
        model,
        move |_, _| {
            model.toggle_pin_playlist(&id);
        }
    ));
    action
}

pub fn build_playlist_actions(id: &str, model: &Rc<SidebarModel>) -> SimpleActionGroup {
    let group = SimpleActionGroup::new();
    group.add_action(&make_play_action(id, model));
    group.add_action(&make_shuffle_action(id, model));
    group.add_action(&make_copy_link_action(id));
    group.add_action(&make_unfollow_action(id, model));
    if is_enabled(FeatureFlag::PinnedObjects) {
        group.add_action(&make_toggle_pin_action(id, model));
    }
    group
}

pub fn build_playlist_menu(is_owned: bool, id: &str, user_id: Option<&str>) -> gio::Menu {
    let playback_section = gio::Menu::new();
    playback_section.append(Some(&*labels::PLAY), Some("playlist.play"));
    playback_section.append(Some(&*labels::SHUFFLE), Some("playlist.shuffle"));

    let delete_section = gio::Menu::new();
    if is_owned {
        delete_section.append(Some(&*labels::DELETE_PLAYLIST), Some("playlist.unfollow"));
    } else {
        delete_section.append(Some(&*labels::UNFOLLOW_PLAYLIST), Some("playlist.unfollow"));
    }

    let pin_section = gio::Menu::new();
    if is_enabled(FeatureFlag::PinnedObjects) {
        let is_pinned = user_id.is_some_and(|user_id| {
            settings::is_object_pinned(user_id, id, settings::PinnedKind::Playlist)
        });
        let pin_label = if is_pinned {
            gettextrs::gettext("Unpin Playlist")
        } else {
            gettextrs::gettext("Pin Playlist")
        };
        pin_section.append(Some(&pin_label), Some("playlist.toggle_pin"));
    }

    let link_section = gio::Menu::new();
    link_section.append(Some(&*labels::COPY_LINK), Some("playlist.copy_link"));

    let menu = gio::Menu::new();
    menu.append_section(None, &playback_section);
    menu.append_section(None, &delete_section);
    if is_enabled(FeatureFlag::PinnedObjects) {
        menu.append_section(None, &pin_section);
    }
    menu.append_section(None, &link_section);
    menu
}

/// The action prefix, actions and menu for a sidebar row's context menu, or
/// `None` for rows without one.
pub fn build_context_menu(
    destination: &SidebarDestination,
    model: &Rc<SidebarModel>,
) -> Option<(&'static str, SimpleActionGroup, gio::Menu)> {
    let (kind, id) = match destination {
        SidebarDestination::Playlist(PlaylistSummary { id, .. }) => {
            let actions = build_playlist_actions(id, model);
            let is_owned = model.is_playlist_owned(id);
            let user_id = model.logged_user_id();
            let menu = build_playlist_menu(is_owned, id, user_id.as_deref());
            return Some(("playlist", actions, menu));
        }
        SidebarDestination::Album { id, .. } => (settings::PinnedKind::Album, id),
        SidebarDestination::Artist { id, .. } => (settings::PinnedKind::Artist, id),
        SidebarDestination::Track { id, .. } => (settings::PinnedKind::Track, id),
        _ => return None,
    };
    let actions = build_pinned_actions(kind, id, model);
    let menu = build_pinned_menu(kind, model.is_in_library(kind, id));
    Some(("pinned", actions, menu))
}

fn action(name: &str, activate: impl Fn() + 'static) -> gio::SimpleAction {
    let action = gio::SimpleAction::new(name, None);
    action.connect_activate(move |_, _| activate());
    action
}

/// Actions for a pinned album, artist or track row, under the `pinned` prefix.
pub fn build_pinned_actions(
    kind: settings::PinnedKind,
    id: &str,
    model: &Rc<SidebarModel>,
) -> SimpleActionGroup {
    let group = SimpleActionGroup::new();
    let play = move |model: &SidebarModel, id: String, shuffle: bool| match kind {
        settings::PinnedKind::Album => model.play_album(id, shuffle),
        settings::PinnedKind::Artist => model.play_artist(id, shuffle),
        _ => model.play_track(id),
    };
    for (name, shuffle) in [("play", false), ("shuffle", true)] {
        let id = id.to_owned();
        group.add_action(&action(
            name,
            clone!(
                #[weak]
                model,
                move || play(&model, id.clone(), shuffle)
            ),
        ));
    }

    let remove_id = id.to_owned();
    group.add_action(&action(
        "remove",
        clone!(
            #[weak]
            model,
            move || model.remove_from_library(kind, remove_id.clone())
        ),
    ));

    let unpin_id = id.to_owned();
    group.add_action(&action(
        "unpin",
        clone!(
            #[weak]
            model,
            move || model.unpin(kind, &unpin_id)
        ),
    ));

    let path = match kind {
        settings::PinnedKind::Playlist => "playlist",
        settings::PinnedKind::Album => "album",
        settings::PinnedKind::Artist => "artist",
        settings::PinnedKind::Track => "track",
    };
    let link = format!("https://open.spotify.com/{path}/{id}");
    group.add_action(&action("copy_link", move || {
        crate::app::components::copy_link_to_clipboard(&link);
    }));
    group
}

/// Menu for a pinned album, artist or track row, laid out like the playlist
/// menu. The remove entry is only shown for items in the user's library.
pub fn build_pinned_menu(kind: settings::PinnedKind, in_library: bool) -> gio::Menu {
    let playback_section = gio::Menu::new();
    playback_section.append(Some(&*labels::PLAY), Some("pinned.play"));
    // Shuffling a single track does nothing useful.
    if kind != settings::PinnedKind::Track {
        playback_section.append(Some(&*labels::SHUFFLE), Some("pinned.shuffle"));
    }

    let remove_section = gio::Menu::new();
    if in_library {
        let label = match kind {
            settings::PinnedKind::Album => &*labels::UNLIKE_ALBUM,
            settings::PinnedKind::Artist => &*labels::UNLIKE_ARTIST,
            _ => &*labels::UNLIKE,
        };
        remove_section.append(Some(label), Some("pinned.remove"));
    }

    let pin_section = gio::Menu::new();
    let unpin_label = match kind {
        settings::PinnedKind::Album => gettextrs::gettext("Unpin Album"),
        settings::PinnedKind::Artist => gettextrs::gettext("Unpin Artist"),
        _ => gettextrs::gettext("Unpin Track"),
    };
    pin_section.append(Some(&unpin_label), Some("pinned.unpin"));

    let link_section = gio::Menu::new();
    link_section.append(Some(&*labels::COPY_LINK), Some("pinned.copy_link"));

    let menu = gio::Menu::new();
    menu.append_section(None, &playback_section);
    if remove_section.n_items() > 0 {
        menu.append_section(None, &remove_section);
    }
    menu.append_section(None, &pin_section);
    menu.append_section(None, &link_section);
    menu
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Action names of each section of `menu`, in order.
    fn menu_actions(menu: &gio::Menu) -> Vec<Vec<String>> {
        (0..menu.n_items())
            .map(|i| {
                let section = menu
                    .item_link(i, gio::MENU_LINK_SECTION)
                    .expect("menu items are sections");
                (0..section.n_items())
                    .filter_map(|j| {
                        section
                            .item_attribute_value(j, gio::MENU_ATTRIBUTE_ACTION, None)
                            .and_then(|v| v.get::<String>())
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn pinned_album_menu_mirrors_playlist_menu() {
        let menu = build_pinned_menu(settings::PinnedKind::Album, true);
        assert_eq!(
            menu_actions(&menu),
            vec![
                vec!["pinned.play", "pinned.shuffle"],
                vec!["pinned.remove"],
                vec!["pinned.unpin"],
                vec!["pinned.copy_link"],
            ]
        );
    }

    #[test]
    fn pinned_track_menu_has_no_shuffle() {
        let menu = build_pinned_menu(settings::PinnedKind::Track, true);
        assert_eq!(menu_actions(&menu)[0], vec!["pinned.play"]);
    }

    #[test]
    fn pinned_menu_omits_remove_outside_library() {
        let menu = build_pinned_menu(settings::PinnedKind::Artist, false);
        assert!(!menu_actions(&menu)
            .concat()
            .contains(&"pinned.remove".to_string()));
    }
}
