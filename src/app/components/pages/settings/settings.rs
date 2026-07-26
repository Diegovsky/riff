use crate::app::components::EventListener;
use crate::app::AppEvent;
use crate::feature_flags::{self, FeatureFlag};
use crate::settings::RiffSettings;

use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use libadwaita::prelude::*;

use super::EqualizerWidget;
use super::PanWidget;
use super::PitchWidget;
use super::SettingsModel;

const SETTINGS: &str = "dev.diegovsky.Riff";

mod imp {

    use super::*;
    use libadwaita::subclass::prelude::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/settings.ui")]
    pub struct SettingsDialog {
        #[template_child]
        pub player_bitrate: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub alsa_device: TemplateChild<gtk::Entry>,

        #[template_child]
        pub alsa_device_row: TemplateChild<libadwaita::ActionRow>,

        #[template_child]
        pub audio_backend: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub gapless_playback: TemplateChild<libadwaita::ActionRow>,

        #[template_child]
        pub ap_port: TemplateChild<gtk::Entry>,

        #[template_child]
        pub theme: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub close_behavior: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub inhibit_suspend_switch: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub skip_explicit_switch: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub volume_curve: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub mono_audio_switch: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub audio_format: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub normalisation_group: TemplateChild<libadwaita::PreferencesGroup>,

        #[template_child]
        pub normalisation_switch: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub normalisation_type: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub normalisation_method: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub normalisation_pregain: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub normalisation_threshold: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub normalisation_attack: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub normalisation_release: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub normalisation_knee: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub equalizer: TemplateChild<EqualizerWidget>,

        #[template_child]
        pub pan: TemplateChild<PanWidget>,

        #[template_child]
        pub pitch: TemplateChild<PitchWidget>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SettingsDialog {
        const NAME: &'static str = "SettingsWindow";
        type Type = super::SettingsDialog;
        type ParentType = libadwaita::PreferencesDialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SettingsDialog {}
    impl WidgetImpl for SettingsDialog {}
    impl AdwDialogImpl for SettingsDialog {}
    impl PreferencesDialogImpl for SettingsDialog {}
}

glib::wrapper! {
    pub struct SettingsDialog(ObjectSubclass<imp::SettingsDialog>) @extends gtk::Widget, libadwaita::Dialog, libadwaita::PreferencesDialog;
}

impl Default for SettingsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsDialog {
    pub fn new() -> Self {
        let dialog: Self = glib::Object::new();

        dialog.bind_backend_and_device();
        dialog.bind_settings();
        dialog.bind_feature_flags();
        dialog.apply_feature_flag_visibility();
        dialog.connect_theme_select();
        dialog
    }

    fn apply_feature_flag_visibility(&self) {
        let widget = self.imp();
        widget
            .normalisation_group
            .set_visible(feature_flags::is_enabled(FeatureFlag::Normalisation));
    }

    fn bind_backend_and_device(&self) {
        let widget = self.imp();

        let audio_backend = widget
            .audio_backend
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        let alsa_device_row = widget
            .alsa_device_row
            .downcast_ref::<libadwaita::ActionRow>()
            .unwrap();

        audio_backend
            .bind_property("selected", alsa_device_row, "visible")
            .transform_to(|_, value: u32| Some(value == 1))
            .build();

        if audio_backend.selected() == 0 {
            alsa_device_row.set_visible(false);
        }
    }

    fn bind_settings(&self) {
        let widget = self.imp();
        let settings = gio::Settings::new(SETTINGS);

        // Binds a GtkAdjustment (from a SpinRow or Scale) to a double GSettings key.
        //
        // We deliberately avoid `Settings::bind` here: its bidirectional binding
        // causes a feedback loop with spin/scale widgets where a single step is
        // written to GSettings and then immediately reverted by the `changed`
        // write-back. Instead we load the initial value and write on every change.
        let bind_double_adjustment = |key: &str, adjustment: &gtk::Adjustment| {
            adjustment.set_value(settings.double(key));
            let settings = settings.clone();
            let key = key.to_owned();
            adjustment.connect_value_changed(move |adj| {
                let _ = settings.set_double(&key, adj.value());
            });
        };

        let player_bitrate = widget
            .player_bitrate
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind("player-bitrate", player_bitrate, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "96" => 0,
                        "160" => 1,
                        "320" => 2,
                        _ => unreachable!(),
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "96",
                        1 => "160",
                        2 => "320",
                        _ => unreachable!(),
                    }
                    .to_variant()
                })
            })
            .build();

        let alsa_device = widget.alsa_device.downcast_ref::<gtk::Entry>().unwrap();
        settings.bind("alsa-device", alsa_device, "text").build();

        let audio_backend = widget
            .audio_backend
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind("audio-backend", audio_backend, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "pulseaudio" => 0,
                        "alsa" => 1,
                        "gstreamer" => 2,
                        _ => unreachable!(),
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "pulseaudio",
                        1 => "alsa",
                        2 => "gstreamer",
                        _ => unreachable!(),
                    }
                    .to_variant()
                })
            })
            .build();

        let gapless_playback = widget
            .gapless_playback
            .downcast_ref::<libadwaita::ActionRow>()
            .unwrap();
        settings
            .bind(
                "gapless-playback",
                &gapless_playback.activatable_widget().unwrap(),
                "active",
            )
            .build();

        let ap_port = widget.ap_port.downcast_ref::<gtk::Entry>().unwrap();
        settings
            .bind("ap-port", ap_port, "text")
            .mapping(|variant, _| variant.get::<u32>().map(|s| s.to_value()))
            .set_mapping(|value, _| value.get::<u32>().ok().map(|u| u.to_variant()))
            .build();

        // Volume curve
        let volume_curve = widget
            .volume_curve
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind("volume-curve", volume_curve, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "log" => 0,
                        "linear" => 1,
                        "cubic" => 2,
                        _ => 0,
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "log",
                        1 => "linear",
                        2 => "cubic",
                        _ => "log",
                    }
                    .to_variant()
                })
            })
            .build();

        // Mono audio
        let mono_audio_switch = widget
            .mono_audio_switch
            .downcast_ref::<libadwaita::SwitchRow>()
            .unwrap();
        settings
            .bind("mono-audio", mono_audio_switch, "active")
            .build();

        // Audio format
        let audio_format = widget
            .audio_format
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind("audio-format", audio_format, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "s16" => 0,
                        "s24" => 1,
                        "s24_3" => 2,
                        "s32" => 3,
                        "f32" => 4,
                        "f64" => 5,
                        _ => 0,
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "s16",
                        1 => "s24",
                        2 => "s24_3",
                        3 => "s32",
                        4 => "f32",
                        5 => "f64",
                        _ => "s16",
                    }
                    .to_variant()
                })
            })
            .build();

        // Normalisation
        let normalisation_switch = widget
            .normalisation_switch
            .downcast_ref::<libadwaita::SwitchRow>()
            .unwrap();
        settings
            .bind("normalisation", normalisation_switch, "active")
            .build();

        let normalisation_type = widget
            .normalisation_type
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind("normalisation-type", normalisation_type, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "auto" => 0,
                        "track" => 1,
                        "album" => 2,
                        _ => 0,
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "auto",
                        1 => "track",
                        2 => "album",
                        _ => "auto",
                    }
                    .to_variant()
                })
            })
            .build();

        let normalisation_method = widget
            .normalisation_method
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind("normalisation-method", normalisation_method, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "dynamic" => 0,
                        "basic" => 1,
                        _ => 0,
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "dynamic",
                        1 => "basic",
                        _ => "dynamic",
                    }
                    .to_variant()
                })
            })
            .build();

        let normalisation_pregain = widget
            .normalisation_pregain
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        bind_double_adjustment(
            "normalisation-pregain-db",
            &normalisation_pregain.adjustment(),
        );

        let normalisation_threshold = widget
            .normalisation_threshold
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        bind_double_adjustment(
            "normalisation-threshold-dbfs",
            &normalisation_threshold.adjustment(),
        );

        let normalisation_attack = widget
            .normalisation_attack
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        bind_double_adjustment(
            "normalisation-attack-ms",
            &normalisation_attack.adjustment(),
        );

        let normalisation_release = widget
            .normalisation_release
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        bind_double_adjustment(
            "normalisation-release-ms",
            &normalisation_release.adjustment(),
        );

        let normalisation_knee = widget
            .normalisation_knee
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        bind_double_adjustment("normalisation-knee-db", &normalisation_knee.adjustment());

        let theme = widget.theme.downcast_ref::<libadwaita::ComboRow>().unwrap();
        settings
            .bind("theme-preference", theme, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "light" => 0,
                        "dark" => 1,
                        "system" => 2,
                        _ => unreachable!(),
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "light",
                        1 => "dark",
                        2 => "system",
                        _ => unreachable!(),
                    }
                    .to_variant()
                })
            })
            .build();

        let close_behavior = widget
            .close_behavior
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind("close-window-behavior", close_behavior, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "ask" => 0,
                        "minimize-to-background" => 1,
                        "stop-and-quit" => 2,
                        _ => unreachable!(),
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "ask",
                        1 => "minimize-to-background",
                        2 => "stop-and-quit",
                        _ => unreachable!(),
                    }
                    .to_variant()
                })
            })
            .build();

        // Keep device awake while playing (inhibit automatic system suspend).
        let inhibit_suspend_switch = widget
            .inhibit_suspend_switch
            .downcast_ref::<libadwaita::SwitchRow>()
            .expect("inhibit_suspend_switch must be a SwitchRow");
        settings
            .bind("inhibit-suspend", inhibit_suspend_switch, "active")
            .build();

        // Skip explicit tracks (local preference). When the account locks the
        // filter, `show_self` overrides this switch to on and disables it.
        let skip_explicit_switch = widget
            .skip_explicit_switch
            .downcast_ref::<libadwaita::SwitchRow>()
            .expect("skip_explicit_switch must be a SwitchRow");
        settings
            .bind("skip-explicit", skip_explicit_switch, "active")
            .build();
    }

    fn bind_feature_flags(&self) {
        let settings = gio::Settings::new(SETTINGS);
        let group = libadwaita::PreferencesGroup::new();
        group.set_title("Experimental Features");
        group.set_description(Some(
            "These settings require restarting the application to take effect.",
        ));

        for flag in FeatureFlag::ALL.iter() {
            let row = libadwaita::SwitchRow::new();
            row.set_title(flag.title());
            row.set_subtitle(flag.description());
            settings.bind(flag.key(), &row, "active").build();
            group.add(&row);
        }

        let page = self
            .upcast_ref::<libadwaita::PreferencesDialog>()
            .visible_page()
            .unwrap();
        page.add(&group);
    }

    fn connect_theme_select(&self) {
        let widget = self.imp();
        let theme = widget.theme.downcast_ref::<libadwaita::ComboRow>().unwrap();
        theme.connect_selected_notify(|theme| {
            debug!("Theme switched! --> value: {}", theme.selected());
            let manager = libadwaita::StyleManager::default();

            let pref = match theme.selected() {
                0 => libadwaita::ColorScheme::ForceLight,
                1 => libadwaita::ColorScheme::ForceDark,
                _ => libadwaita::ColorScheme::Default,
            };

            manager.set_color_scheme(pref);
        });
    }

    fn connect_close<F>(&self, on_close: F)
    where
        F: Fn() + 'static,
    {
        let dialog = self.upcast_ref::<libadwaita::Dialog>();
        dialog.connect_close_attempt(move |_| {
            on_close();
        });
    }

    /// Re-lock the equalizer and pan controls. The locks are UI-only and never
    /// persisted, so they are reset every time the dialog is opened.
    fn reset_locks(&self) {
        self.imp().equalizer.lock();
        self.imp().pan.lock();
        self.imp().pitch.lock();
    }

    /// Apply the Spotify account's explicit content filter lock to the toggle.
    ///
    /// When the account has the filter locked (e.g. a family plan parental
    /// control), the switch is forced on, made insensitive so it cannot be
    /// changed, and given a tooltip explaining why. Otherwise the switch is
    /// interactive and reflects the local preference.
    fn set_explicit_filter_locked(&self, locked: bool) {
        let switch = &self.imp().skip_explicit_switch;
        if locked {
            switch.set_active(true);
            switch.set_sensitive(false);
            switch.set_tooltip_text(Some(&gettextrs::gettext(
                "Locked by a Family plan parental control and cannot be changed here.",
            )));
        } else {
            switch.set_sensitive(true);
            switch.set_tooltip_text(None);
        }
    }
}

pub struct Settings {
    parent: gtk::Window,
    settings_dialog: SettingsDialog,
    model: Rc<SettingsModel>,
}

impl Settings {
    pub fn new(parent: gtk::Window, model: SettingsModel) -> Self {
        let settings_dialog = SettingsDialog::new();
        let model = Rc::new(model);

        let close_model = model.clone();
        settings_dialog.connect_close(move || {
            let new_settings = RiffSettings::new_from_gsettings().unwrap_or_default();
            // Only stop the player for changes that require a full reload.
            // Equalizer changes are applied live and must not interrupt playback.
            if close_model
                .settings()
                .player_settings
                .requires_reload(&new_settings.player_settings)
            {
                close_model.stop_player();
            }
            close_model.set_settings();
        });

        Self {
            parent,
            settings_dialog,
            model,
        }
    }

    fn dialog(&self) -> &libadwaita::Dialog {
        self.settings_dialog.upcast_ref::<libadwaita::Dialog>()
    }

    pub fn show_self(&self) {
        // Locks are UI-only and must not be stateful between openings.
        self.settings_dialog.reset_locks();
        // Reflect the account's explicit content filter lock each time the
        // dialog opens, since it becomes known only after login.
        self.settings_dialog
            .set_explicit_filter_locked(self.model.explicit_filter_locked());
        self.dialog().present(Some(&self.parent));
    }
}

impl EventListener for Settings {
    fn on_event(&mut self, _: &AppEvent) {}
}
