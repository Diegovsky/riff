mod shell_box;
pub use shell_box::expose_widgets;

use gettextrs::gettext;
use gio::prelude::SettingsExt;
use gtk::prelude::*;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::components::EventListener;
use crate::app::{AppEvent, AppModel};
use crate::settings::{CloseWindowBehavior, WindowGeometry};

const SETTINGS: &str = "dev.diegovsky.Riff";

thread_local! {
    static WINDOW_GEOMETRY: RefCell<WindowGeometry> = const { RefCell::new(WindowGeometry {
        width: 0, height: 0, is_maximized: false
    }) };
}

pub struct MainWindow {
    initial_window_geometry: WindowGeometry,
    window: libadwaita::ApplicationWindow,
}

impl MainWindow {
    pub fn new(
        initial_window_geometry: WindowGeometry,
        app_model: Rc<AppModel>,
        window: libadwaita::ApplicationWindow,
    ) -> Self {
        window.connect_close_request(clone!(
            #[weak]
            app_model,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |window| {
                let state = app_model.get_state();
                if !state.playback.is_playing() {
                    return glib::Propagation::Proceed;
                }

                let settings = gio::Settings::new(SETTINGS);
                let behavior = CloseWindowBehavior::from_gsettings_enum(
                    settings.enum_("close-window-behavior"),
                );

                match behavior {
                    CloseWindowBehavior::MinimizeToBackground => {
                        window.set_visible(false);
                        glib::Propagation::Stop
                    }
                    CloseWindowBehavior::StopAndQuit => glib::Propagation::Proceed,
                    CloseWindowBehavior::Ask => {
                        Self::show_close_dialog(window);
                        glib::Propagation::Stop
                    }
                }
            }
        ));

        window.connect_default_height_notify(Self::save_window_geometry);
        window.connect_default_width_notify(Self::save_window_geometry);
        window.connect_maximized_notify(Self::save_window_geometry);

        window.connect_unrealize(|_| {
            debug!("saving geometry");
            WINDOW_GEOMETRY.with(|g| g.borrow().save());
        });

        Self {
            initial_window_geometry,
            window,
        }
    }

    fn show_close_dialog(window: &libadwaita::ApplicationWindow) {
        let dialog = libadwaita::AlertDialog::new(
            Some(&gettext("Riff is still playing")),
            Some(&gettext(
                "What should Riff do if you close it while audio is playing?",
            )),
        );

        dialog.add_response("background", &gettext("Continue in background"));
        dialog.add_response("quit", &gettext("Stop audio and quit"));

        dialog.set_response_appearance("quit", libadwaita::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("background"));
        dialog.set_close_response("background");
        dialog.set_prefer_wide_layout(true);

        let remember_check = gtk::CheckButton::with_label(&gettext("Remember my choice"));
        remember_check.set_halign(gtk::Align::Center);
        dialog.set_extra_child(Some(&remember_check));

        dialog.choose(
            window,
            None::<&gio::Cancellable>,
            clone!(
                #[weak]
                window,
                move |response| {
                    let settings = gio::Settings::new(SETTINGS);
                    match response.as_str() {
                        "quit" => {
                            if remember_check.is_active() {
                                let _ = settings.set_enum(
                                    "close-window-behavior",
                                    CloseWindowBehavior::StopAndQuit as i32,
                                );
                            }
                            if let Some(app) = window.application() {
                                app.quit();
                            }
                        }
                        _ => {
                            // "background" or dialog dismissed
                            if remember_check.is_active() {
                                let _ = settings.set_enum(
                                    "close-window-behavior",
                                    CloseWindowBehavior::MinimizeToBackground as i32,
                                );
                            }
                            window.set_visible(false);
                        }
                    }
                }
            ),
        );
    }

    fn start(&self) {
        self.window.set_default_size(
            self.initial_window_geometry.width,
            self.initial_window_geometry.height,
        );
        if self.initial_window_geometry.is_maximized {
            self.window.maximize();
        }
        self.window.present();
    }

    fn raise(&self) {
        self.window.present();
    }

    fn save_window_geometry<W: GtkWindowExt>(window: &W) {
        let (width, height) = window.default_size();
        let is_maximized = window.is_maximized();
        WINDOW_GEOMETRY.with(|g| {
            let mut g = g.borrow_mut();
            g.is_maximized = is_maximized;
            if !is_maximized {
                g.width = width;
                g.height = height;
            }
        });
    }
}

impl EventListener for MainWindow {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::Started => self.start(),
            AppEvent::Raised => self.raise(),
            _ => {}
        }
    }
}
