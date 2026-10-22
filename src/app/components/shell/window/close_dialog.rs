use gettextrs::gettext;
use gio::prelude::SettingsExt;
use gtk::prelude::*;
use libadwaita::prelude::*;

use crate::settings::CloseWindowBehavior;

use riff_config::settings::SETTINGS;

pub fn show(window: &libadwaita::ApplicationWindow) {
    let dialog = libadwaita::AlertDialog::new(
        Some(&gettext("Riff is still playing")),
        Some(&gettext(
            "What should Riff do if you close it while audio is playing?",
        )),
    );

    dialog.add_response("background", &gettext("Continue in Background"));
    dialog.add_response("quit", &gettext("Stop Audio and Quit"));

    dialog.set_response_appearance("quit", libadwaita::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("background"));
    dialog.set_close_response("dismiss");
    dialog.set_prefer_wide_layout(true);

    let remember_check = gtk::CheckButton::with_label(&gettext("Remember my choice"));
    remember_check.set_halign(gtk::Align::Center);
    dialog.set_extra_child(Some(&remember_check));

    add_close_button(&dialog);

    dialog.choose(
        Some(window),
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
                    "background" => {
                        if remember_check.is_active() {
                            let _ = settings.set_enum(
                                "close-window-behavior",
                                CloseWindowBehavior::MinimizeToBackground as i32,
                            );
                        }
                        window.set_visible(false);
                    }
                    _ => {}
                }
            }
        ),
    );
}

fn add_close_button(dialog: &libadwaita::AlertDialog) {
    let Some(contents) = dialog.child() else {
        return;
    };
    dialog.set_child(None::<&gtk::Widget>);

    let close_button = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text(gettext("Close"))
        .halign(gtk::Align::End)
        .valign(gtk::Align::Start)
        .margin_top(6)
        .margin_end(6)
        .build();
    close_button.add_css_class("circular");
    close_button.add_css_class("flat");
    close_button.connect_clicked(clone!(
        #[weak]
        dialog,
        move |_| {
            dialog.close();
        }
    ));

    let overlay = gtk::Overlay::builder().child(&contents).build();
    overlay.add_overlay(&close_button);
    dialog.set_child(Some(&overlay));
}
