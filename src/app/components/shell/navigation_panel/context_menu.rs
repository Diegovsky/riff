use gdk::prelude::*;
use gio::SimpleActionGroup;
use std::rc::Rc;

use super::{NavigationPanelDestination, NavigationPanelModel};
use crate::app::components::labels;
use crate::app::components::utils::confirm_delete_playlist;
use crate::app::models::PlaylistSummary;
use crate::settings;
use riff_config::feature_flags::{is_enabled, FeatureFlag};

fn make_play_action(id: &str, model: &Rc<NavigationPanelModel>) -> gio::SimpleAction {
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

fn make_shuffle_action(id: &str, model: &Rc<NavigationPanelModel>) -> gio::SimpleAction {
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

fn make_queue_action(id: &str, model: &Rc<NavigationPanelModel>) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("queue", None);
    let id = id.to_owned();
    action.connect_activate(clone!(
        #[weak]
        model,
        move |_, _| {
            model.queue_playlist(id.clone());
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

fn confirm_unfollow_playlist(model: &Rc<NavigationPanelModel>, id: String) {
    if !model.is_playlist_owned(&id) {
        model.unfollow_playlist(id);
        return;
    }
    confirm_delete_playlist(
        None,
        clone!(
            #[weak]
            model,
            move || model.unfollow_playlist(id)
        ),
    );
}

fn make_unfollow_action(id: &str, model: &Rc<NavigationPanelModel>) -> gio::SimpleAction {
    let action = gio::SimpleAction::new("unfollow", None);
    let id = id.to_owned();
    action.connect_activate(clone!(
        #[weak]
        model,
        move |_, _| confirm_unfollow_playlist(&model, id.clone())
    ));
    action
}

fn make_toggle_pin_action(id: &str, model: &Rc<NavigationPanelModel>) -> gio::SimpleAction {
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

pub fn build_playlist_actions(id: &str, model: &Rc<NavigationPanelModel>) -> SimpleActionGroup {
    let group = SimpleActionGroup::new();
    group.add_action(&make_play_action(id, model));
    group.add_action(&make_shuffle_action(id, model));
    group.add_action(&make_queue_action(id, model));
    group.add_action(&make_copy_link_action(id));
    group.add_action(&make_unfollow_action(id, model));
    if is_enabled(FeatureFlag::PinnedObjects) {
        group.add_action(&make_toggle_pin_action(id, model));
    }
    group
}

/// Laid out like the track list's menu: the library, pin and queue entries
/// share a section, in that order, and the link comes last.
pub fn build_playlist_menu(is_owned: bool, id: &str, user_id: Option<&str>) -> gio::Menu {
    let playback_section = gio::Menu::new();
    playback_section.append(Some(&*labels::PLAY), Some("playlist.play"));
    playback_section.append(Some(&*labels::SHUFFLE), Some("playlist.shuffle"));

    let playlist_section = gio::Menu::new();
    let remove_label = if is_owned {
        &*labels::DELETE_PLAYLIST
    } else {
        &*labels::UNFOLLOW_PLAYLIST
    };
    playlist_section.append(Some(remove_label), Some("playlist.unfollow"));
    if is_enabled(FeatureFlag::PinnedObjects) {
        let is_pinned = user_id.is_some_and(|user_id| {
            settings::is_object_pinned(user_id, id, settings::PinnedKind::Playlist)
        });
        let pin_label = if is_pinned {
            &*labels::UNPIN_FROM_SIDEBAR
        } else {
            &*labels::PIN_TO_SIDEBAR
        };
        playlist_section.append(Some(pin_label), Some("playlist.toggle_pin"));
    }
    playlist_section.append(Some(&*labels::ADD_TO_QUEUE), Some("playlist.queue"));

    let link_section = gio::Menu::new();
    link_section.append(Some(&*labels::COPY_LINK), Some("playlist.copy_link"));

    let menu = gio::Menu::new();
    menu.append_section(None, &playback_section);
    menu.append_section(None, &playlist_section);
    menu.append_section(None, &link_section);
    menu
}

/// The action prefix, actions and menu for a navigation panel row's context menu, or
/// `None` for rows without one.
pub fn build_context_menu(
    destination: &NavigationPanelDestination,
    model: &Rc<NavigationPanelModel>,
) -> Option<(&'static str, SimpleActionGroup, gio::Menu)> {
    let (kind, id) = match destination {
        NavigationPanelDestination::Playlist(PlaylistSummary { id, .. }) => {
            let actions = build_playlist_actions(id, model);
            let is_owned = model.is_playlist_owned(id);
            let user_id = model.logged_user_id();
            let menu = build_playlist_menu(is_owned, id, user_id.as_deref());
            return Some(("playlist", actions, menu));
        }
        NavigationPanelDestination::Album { id, .. } => (settings::PinnedKind::Album, id),
        NavigationPanelDestination::Artist { id, .. } => (settings::PinnedKind::Artist, id),
        NavigationPanelDestination::Track { id, .. } => (settings::PinnedKind::Track, id),
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
    model: &Rc<NavigationPanelModel>,
) -> SimpleActionGroup {
    let group = SimpleActionGroup::new();
    let play = move |model: &NavigationPanelModel, id: String, shuffle: bool| match kind {
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
            move || {
                if kind == settings::PinnedKind::Playlist {
                    confirm_unfollow_playlist(&model, remove_id.clone());
                } else {
                    model.remove_from_library(kind, remove_id.clone());
                }
            }
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

    let item_section = gio::Menu::new();
    if in_library {
        let label = match kind {
            settings::PinnedKind::Album => &*labels::UNLIKE_ALBUM,
            settings::PinnedKind::Artist => &*labels::UNLIKE_ARTIST,
            _ => &*labels::UNLIKE,
        };
        item_section.append(Some(label), Some("pinned.remove"));
    }
    item_section.append(Some(&*labels::UNPIN_FROM_SIDEBAR), Some("pinned.unpin"));

    let link_section = gio::Menu::new();
    link_section.append(Some(&*labels::COPY_LINK), Some("pinned.copy_link"));

    let menu = gio::Menu::new();
    menu.append_section(None, &playback_section);
    menu.append_section(None, &item_section);
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
                vec!["pinned.remove", "pinned.unpin"],
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
