#![allow(mismatched_lifetime_syntaxes)]
#[macro_use(clone)]
extern crate glib;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate gettextrs;

use app::state::{HomePage, Route};
use futures::channel::mpsc::UnboundedSender;
use gettextrs::*;
use gio::prelude::*;
use gio::ApplicationFlags;
use gio::SimpleAction;
use gtk::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

mod app;
mod config;
mod connect;
mod dbus;
mod inhibitor;
mod player;
mod settings;

use crate::app::components::expose_custom_widgets;
use crate::app::dispatch::DispatchLoop;
use crate::app::{state::PlaybackAction, App, AppAction, BrowserAction};

// Steps of the seek and volume shortcuts
const SEEK_STEP_MS: i32 = 5_000;
const VOLUME_STEP: f64 = 0.05;
#[cfg(target_env = "gnu")]
const MALLOC_ARENAS: libc::c_int = 4;

fn main() {
    #[cfg(target_env = "gnu")]
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, MALLOC_ARENAS);
    }

    let settings = settings::RiffSettings::new_from_gsettings().unwrap_or_default();
    setup_gtk(&settings);

    // Looks like there's a side effect to declaring widgets that allows them to be referenced them in ui/blueprint files
    // so here goes!
    expose_custom_widgets();

    let gtk_app = gtk::Application::new(Some(config::APPID), ApplicationFlags::HANDLES_OPEN);
    let builder = gtk::Builder::from_resource("/dev/diegovsky/Riff/window.ui");
    let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();

    // In debug mode, the app id is different (see meson config) so we fix the resource path (and add a distinctive style)
    // Having a different app id allows running both the stable and development version at the same time
    if cfg!(debug_assertions) {
        window.add_css_class("devel");
        gtk_app.set_resource_base_path(Some("/dev/diegovsky/Riff"));
    }

    let context = glib::MainContext::default();
    let dispatch_loop = DispatchLoop::new();
    let sender = dispatch_loop.make_dispatcher();

    // Couple of actions used with shortcuts
    register_actions(&gtk_app, sender.clone());
    // Displayed as the accelerator hint next to "Preferences" in the main menu.
    gtk_app.set_accels_for_action("win.preferences", &["<Ctrl>comma"]);
    // Displayed as the accelerator hint next to "Keyboard Shortcuts" in the main menu.
    gtk_app.set_accels_for_action("win.show-shortcuts", &["<Ctrl>question"]);
    app::components::setup_about(builder.object::<libadwaita::AboutDialog>("about").unwrap());
    setup_arrow_shortcuts(&window, builder.object("arrow_shortcuts").unwrap());

    // Main app logic is hooked up here
    let app = App::new(settings, builder, sender.clone());
    context.spawn_local(app.attach(dispatch_loop));

    let sender_clone = sender.clone();
    gtk_app.connect_activate(move |gtk_app| {
        debug!("activate");
        if let Some(existing_window) = gtk_app.active_window() {
            existing_window.present();
        } else {
            // Only send the Start action if we've just created the window
            window.set_application(Some(gtk_app));
            gtk_app.add_window(&window);
            sender_clone.unbounded_send(AppAction::Start).unwrap();
        }
    });

    gtk_app.connect_open(move |gtk_app, targets, _| {
        gtk_app.activate();

        // There should only be one target because %u is used in desktop file
        let Some(target) = targets.first() else {
            return;
        };
        let uri = target.uri().to_string();
        let action = AppAction::OpenURI(uri)
            .unwrap_or_else(|| AppAction::ShowNotification(gettext("Failed to open link!")));
        sender.unbounded_send(action).unwrap();
    });

    context.invoke_local(move || {
        gtk_app.run();
    });

    if RESTART_REQUESTED.load(Ordering::Relaxed) {
        relaunch();
    }

    std::process::exit(0);
}

static RESTART_REQUESTED: AtomicBool = AtomicBool::new(false);

fn relaunch() {
    use std::os::unix::process::CommandExt;

    let Ok(exe) = std::env::current_exe() else {
        error!("Cannot restart: unknown executable path");
        return;
    };
    let err = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .exec();
    error!("Cannot restart: {err}");
}

fn setup_gtk(settings: &settings::RiffSettings) {
    // Setup logging
    env_logger::init();

    // Setup translations
    textdomain("riff")
        .and_then(|_| bindtextdomain("riff", config::LOCALEDIR))
        .and_then(|_| bind_textdomain_codeset("riff", "UTF-8"))
        .expect("Could not setup localization");

    // Setup Gtk, Adwaita...
    gtk::init().unwrap_or_else(|_| panic!("Failed to initialize GTK"));
    libadwaita::init().unwrap_or_else(|_| panic!("Failed to initialize libadwaita"));

    let manager = libadwaita::StyleManager::default();
    manager.set_color_scheme(settings.theme_preference);

    let res = gio::Resource::load(config::PKGDATADIR.to_owned() + "/riff.gresource")
        .expect("Could not load resources");
    gio::resources_register(&res);

    let provider = gtk::CssProvider::new();
    provider.load_from_resource("/dev/diegovsky/Riff/app.css");

    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().unwrap(),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn setup_arrow_shortcuts(
    window: &libadwaita::ApplicationWindow,
    shortcuts: gtk::ShortcutController,
) {
    window.connect_focus_widget_notify(move |window| {
        let typing = GtkWindowExt::focus(window)
            .is_some_and(|focus| focus.is::<gtk::Text>() || focus.is::<gtk::TextView>());
        shortcuts.set_propagation_phase(if typing {
            gtk::PropagationPhase::None
        } else {
            gtk::PropagationPhase::Capture
        });
    });
}

fn register_actions(app: &gtk::Application, sender: UnboundedSender<AppAction>) {
    let quit = SimpleAction::new("quit", None);
    quit.connect_activate(clone!(
        #[weak]
        app,
        move |_, _| {
            if let Some(existing_window) = app.active_window() {
                existing_window.close();
            }
            app.quit();
        }
    ));
    app.add_action(&quit);

    let restart = SimpleAction::new("restart", None);
    restart.connect_activate(clone!(
        #[weak]
        app,
        move |_, _| {
            RESTART_REQUESTED.store(true, Ordering::Relaxed);
            app.quit();
        }
    ));
    app.add_action(&restart);

    let resize_window =
        SimpleAction::new("resize_window", Some(glib::VariantTy::new("(ii)").unwrap()));
    resize_window.connect_activate(clone!(
        #[weak]
        app,
        move |_, parameter| {
            let Some((width, height)) = parameter.and_then(|p| p.get::<(i32, i32)>()) else {
                return;
            };
            if let Some(window) = app.active_window() {
                window.unmaximize();
                window.set_default_size(width, height);
            }
        }
    ));
    app.add_action(&resize_window);

    app.add_action(&make_action(
        "toggle_playback",
        PlaybackAction::TogglePlay.into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "player_prev",
        PlaybackAction::Previous.into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "player_next",
        PlaybackAction::Next.into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "toggle_shuffle",
        PlaybackAction::ToggleShuffle.into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "toggle_repeat",
        PlaybackAction::ToggleRepeat.into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "seek_backward",
        PlaybackAction::SeekBy(-SEEK_STEP_MS).into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "seek_forward",
        PlaybackAction::SeekBy(SEEK_STEP_MS).into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "volume_down",
        PlaybackAction::AdjustVolume(-VOLUME_STEP).into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "volume_up",
        PlaybackAction::AdjustVolume(VOLUME_STEP).into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "toggle_mute",
        PlaybackAction::ToggleMute.into(),
        sender.clone(),
    ));

    for (name, page) in [
        ("nav_now_playing", HomePage::NowPlaying),
        ("nav_artists", HomePage::SavedArtists),
        ("nav_albums", HomePage::Library),
        ("nav_playlists", HomePage::SavedPlaylists),
        ("nav_tracks", HomePage::SavedTracks),
    ] {
        // Same as picking the page in the navigation panel
        app.add_action(&make_action(
            name,
            BrowserAction::Navigate(Route::Home(page)).into(),
            sender.clone(),
        ));
    }

    app.add_action(&make_action(
        "nav_back",
        AppAction::BrowserAction(BrowserAction::NavigationBack),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "nav_forward",
        AppAction::BrowserAction(BrowserAction::NavigationForward),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "search",
        AppAction::BrowserAction(BrowserAction::Navigate(Route::Search)),
        sender.clone(),
    ));
}

fn make_action(
    name: &str,
    app_action: AppAction,
    sender: UnboundedSender<AppAction>,
) -> SimpleAction {
    let action = SimpleAction::new(name, None);
    action.connect_activate(move |_, _| {
        sender.unbounded_send(app_action.clone()).unwrap();
    });
    action
}
