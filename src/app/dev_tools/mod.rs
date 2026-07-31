//! Developer tools menu wiring (debug builds only).
//!
//! This holds the setup for the hidden "dev" menu button and all the switches
//! and buttons it contains. It is only compiled in debug builds and is kept out
//! of `App::add_ui_components` to keep that method focused on the real UI.

use futures::channel::mpsc::UnboundedSender;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

mod panel_sizes;

use super::state::{AppAction, AppModel};
use crate::player::Command;

/// Wire up the dev tools menu and all of its controls.
///
/// Loads the dev menu from its own blueprint (`dev_tools.blp`), packs it into
/// the sidebar header, and connects every dev widget to the app and player
/// senders. Called once during UI setup in debug builds.
pub fn wire_dev_tools(
    builder: &gtk::Builder,
    sender: &UnboundedSender<AppAction>,
    player_command_sender: &UnboundedSender<Command>,
    model: &Rc<AppModel>,
) {
    // The dev menu lives in its own blueprint (src/app/dev_tools.blp) so the
    // dev-only markup stays out of window.blp. Load it here and pack it into
    // the sidebar header, right after the search button.
    let dev_builder = gtk::Builder::from_resource("/dev/diegovsky/Riff/dev_tools.ui");
    let dev_menu: gtk::MenuButton = dev_builder.object("dev_menu").unwrap();
    let sidebar_header: libadwaita::HeaderBar = builder.object("sidebar_header").unwrap();
    sidebar_header.pack_start(&dev_menu);
    dev_menu.set_visible(true);

    let display = gdk::Display::default().unwrap();

    // Force Skeleton: the skeleton override styles only take effect when
    // the window carries the `force-skeleton` class, so the provider can
    // be registered up front and the switch just toggles the class. No
    // restart required.
    let skeleton_provider = gtk::CssProvider::new();
    skeleton_provider.load_from_resource("/dev/diegovsky/Riff/skeleton_override.css");
    gtk::style_context_add_provider_for_display(
        &display,
        &skeleton_provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );

    let dev_skeleton_switch: gtk::Switch = dev_builder.object("dev_skeleton_switch").unwrap();
    let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();
    dev_skeleton_switch.connect_state_set(move |_, active| {
        if active {
            window.add_css_class("force-skeleton");
        } else {
            window.remove_css_class("force-skeleton");
        }
        glib::Propagation::Proceed
    });

    // Debug CSS: alignment/rendering overlays. The provider applies its
    // styles globally, so it is added and removed on demand rather than
    // gated behind a class. No restart required.
    let debug_css_provider = gtk::CssProvider::new();
    debug_css_provider.load_from_resource("/dev/diegovsky/Riff/debug.css");
    let dev_debug_css_switch: gtk::Switch = dev_builder.object("dev_debug_css_switch").unwrap();
    let display_for_debug = display.clone();
    dev_debug_css_switch.connect_state_set(move |_, active| {
        if active {
            gtk::style_context_add_provider_for_display(
                &display_for_debug,
                &debug_css_provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        } else {
            gtk::style_context_remove_provider_for_display(&display_for_debug, &debug_css_provider);
        }
        glib::Propagation::Proceed
    });

    // Panel Sizes: overlay each major panel's pixel dimensions.
    let dev_panel_sizes_switch: gtk::Switch = dev_builder.object("dev_panel_sizes_switch").unwrap();
    panel_sizes::wire(builder, &dev_panel_sizes_switch);

    let dev_offline_switch: gtk::Switch = dev_builder.object("dev_offline_switch").unwrap();
    let player_sender = player_command_sender.clone();
    dev_offline_switch.connect_state_set(move |_, active| {
        // Block real HTTP calls in the API client so browsing, search and
        // library requests fail as if there were no network. When toggled on,
        // also kill the librespot session to mirror a real network drop where
        // the TCP connection to the access point dies.
        //
        // The connection-lost banner is deliberately NOT raised here. It
        // appears as a side effect once an actual API request fails while
        // offline, and clears itself once a request succeeds again (see
        // call_spotify_and_dispatch_many), mirroring how a real outage is
        // detected rather than the switch poking the toast directly.
        crate::api::set_simulate_offline(active);
        if active {
            let _ = player_sender.unbounded_send(Command::DevKillSession);
        }
        glib::Propagation::Proceed
    });

    let dev_kill_player: gtk::Button = dev_builder.object("dev_kill_player").unwrap();
    let player_sender = player_command_sender.clone();
    dev_kill_player.connect_clicked(move |_| {
        let _ = player_sender.unbounded_send(Command::DevKillPlayer);
    });

    let dev_kill_session: gtk::Button = dev_builder.object("dev_kill_session").unwrap();
    let player_sender = player_command_sender.clone();
    dev_kill_session.connect_clicked(move |_| {
        let _ = player_sender.unbounded_send(Command::DevKillSession);
    });

    let dev_simulate_unavailable: gtk::Button =
        dev_builder.object("dev_simulate_unavailable").unwrap();
    let player_sender = player_command_sender.clone();
    dev_simulate_unavailable.connect_clicked(move |_| {
        let _ = player_sender.unbounded_send(Command::DevSimulateTrackUnavailable);
    });

    // Inject API Error: force every Spotify Web API request to fail with
    // the selected error. Uses toggle buttons instead of a DropDown to
    // avoid a GTK4 bug where a DropDown inside a Popover breaks the
    // popover's autohide grab (GNOME/gtk#5568).
    let error_buttons: [gtk::ToggleButton; 5] = [
        dev_builder.object("dev_error_off").unwrap(),
        dev_builder.object("dev_error_429").unwrap(),
        dev_builder.object("dev_error_401").unwrap(),
        dev_builder.object("dev_error_500").unwrap(),
        dev_builder.object("dev_error_nocontent").unwrap(),
    ];
    for (index, button) in error_buttons.iter().enumerate() {
        let idx = index as u8;
        button.connect_toggled(move |btn| {
            if btn.is_active() {
                crate::api::set_injected_error(idx);
            }
        });
    }

    // Expire OAuth Token: backdate the cached token and force a refresh.
    let dev_expire_token: gtk::Button = dev_builder.object("dev_expire_token").unwrap();
    let player_sender = player_command_sender.clone();
    dev_expire_token.connect_clicked(move |_| {
        let _ = player_sender.unbounded_send(Command::DevExpireToken);
    });

    // Test Toast: fire a notification so the toast overlay can be checked.
    let dev_test_toast: gtk::Button = dev_builder.object("dev_test_toast").unwrap();
    let app_sender = sender.clone();
    dev_test_toast.connect_clicked(move |_| {
        let _ = app_sender.unbounded_send(AppAction::ShowNotification(
            "Test toast notification".to_string(),
        ));
    });

    // Reload CSS: reload the stylesheets from their source files on disk
    // (not the bundled gresource copies) so style edits apply live. Providers
    // are created and registered at USER priority lazily on the first click,
    // so nothing is registered unless this tool is actually used, then reused
    // and reloaded in place on every subsequent click.
    let dev_reload_css: gtk::Button = dev_builder.object("dev_reload_css").unwrap();
    let css_sources: [&'static str; 7] = [
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/app.css"),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/app/components/pages/search/search.css"
        ),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/app/components/widgets/card/card.css"
        ),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/app/components/widgets/details_page/style.css"
        ),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/app/components/widgets/playlist/song.css"
        ),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/app/components/shell/playback/playback.css"
        ),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/app/components/widgets/selection/selection_toolbar.css"
        ),
    ];
    let css_providers: RefCell<Vec<gtk::CssProvider>> = RefCell::new(Vec::new());
    let display_for_css = display.clone();
    dev_reload_css.connect_clicked(move |_| {
        let mut providers = css_providers.borrow_mut();
        // First click: create and register one provider per source file.
        if providers.is_empty() {
            for _ in css_sources.iter() {
                let provider = gtk::CssProvider::new();
                gtk::style_context_add_provider_for_display(
                    &display_for_css,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_USER,
                );
                providers.push(provider);
            }
        }
        for (path, provider) in css_sources.iter().zip(providers.iter()) {
            provider.load_from_path(path);
        }
        info!("[dev] Reloaded CSS from source files");
    });

    // Dump State to Log: print a summary of the current app state.
    let dev_dump_state: gtk::Button = dev_builder.object("dev_dump_state").unwrap();
    let model_for_dump = Rc::clone(model);
    dev_dump_state.connect_clicked(move |_| {
        let state = model_for_dump.get_state();
        info!("==== APP STATE DUMP ====");
        info!("logged-in user: {:?}", state.logged_user.user);
        info!("owned playlists: {}", state.logged_user.playlists.len());
        info!(
            "browser screen: {:?} (nav depth {})",
            state.browser.current_screen(),
            state.browser.count()
        );
        info!(
            "selection: enabled={} count={} context={:?}",
            state.selection.is_selection_enabled(),
            state.selection.count(),
            state.selection.context
        );
        // Summarise playback rather than `{:#?}`-printing the whole
        // state: the full Debug output includes every queued track's
        // metadata and the shuffle index, which for a large queue
        // builds a huge string on the main thread and freezes the UI.
        let playback = &state.playback;
        info!(
            "playback: playing={} shuffled={} repeat={:?} current_song={:?} queue_len={}",
            playback.is_playing(),
            playback.is_shuffled(),
            playback.repeat_mode(),
            playback.current_song_id(),
            playback.songs().len()
        );
        info!("==== END STATE DUMP ====");
    });

    // Clear the persisted verified marker and re-arm DRM detection.
    let dev_reset_drm: gtk::Button = dev_builder.object("dev_reset_drm").unwrap();
    let player_sender = player_command_sender.clone();
    dev_reset_drm.connect_clicked(move |_| {
        let _ = player_sender.unbounded_send(Command::DevResetDrmVerification);
    });

    // Show the DRM dialog on demand.
    let dev_show_drm_dialog: gtk::Button = dev_builder.object("dev_show_drm_dialog").unwrap();
    let app_sender = sender.clone();
    dev_show_drm_dialog.connect_clicked(move |_| {
        let _ = app_sender.unbounded_send(AppAction::ShowDrmBlockedDialog);
    });
}
