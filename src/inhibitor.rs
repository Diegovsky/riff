//! Holds a GtkApplication suspend inhibitor while audio is playing, so the
//! system does not automatically suspend during unattended playback.

use gettextrs::gettext;
use gio::prelude::SettingsExt;
use gtk::prelude::*;
use std::rc::Rc;

use crate::app::components::EventListener;
use crate::app::{AppEvent, AppModel};

const SETTINGS: &str = "dev.diegovsky.Riff";
const INHIBIT_SUSPEND_KEY: &str = "inhibit-suspend";

/// Pure decision helper: whether a suspend inhibitor should be held.
///
/// An inhibitor is held only while audio is actively playing and the user has
/// enabled the "keep device awake" preference.
fn should_inhibit(enabled: bool, playing: bool) -> bool {
    enabled && playing
}

/// Holds a GtkApplication suspend inhibitor while audio is playing.
///
/// The inhibitor is acquired when playback is active (and the preference is
/// enabled) and released when playback is paused or stopped, or when the
/// preference is turned off. Inside a Flatpak sandbox, GTK routes this through
/// the org.freedesktop.portal.Inhibit portal transparently.
pub struct SuspendInhibitor {
    window: libadwaita::ApplicationWindow,
    app_model: Rc<AppModel>,
    settings: gio::Settings,
    cookie: Rc<std::cell::Cell<Option<u32>>>,
}

impl SuspendInhibitor {
    pub fn new(window: libadwaita::ApplicationWindow, app_model: Rc<AppModel>) -> Self {
        let settings = gio::Settings::new(SETTINGS);
        let cookie = Rc::new(std::cell::Cell::new(None));

        let inhibitor = Self {
            window,
            app_model,
            settings,
            cookie,
        };

        inhibitor.connect_settings_changed();
        // Establish the correct state up front in case playback is already
        // active when the inhibitor is created.
        inhibitor.refresh();
        inhibitor
    }

    /// React immediately when the preference is toggled, so enabling or
    /// disabling it during playback takes effect without waiting for the next
    /// playback event.
    fn connect_settings_changed(&self) {
        let window = self.window.clone();
        let app_model = Rc::clone(&self.app_model);
        let settings = self.settings.clone();
        let cookie = Rc::clone(&self.cookie);
        self.settings
            .connect_changed(Some(INHIBIT_SUSPEND_KEY), move |_, _| {
                Self::recompute(&window, &settings, &app_model, &cookie);
            });
    }

    /// Recompute the desired inhibit state from the current settings and
    /// playback state, then apply it.
    fn refresh(&self) {
        Self::recompute(&self.window, &self.settings, &self.app_model, &self.cookie);
    }

    /// Read the current preference and playback state and apply the resulting
    /// inhibit decision. Shared by `refresh` and the settings-changed handler.
    fn recompute(
        window: &libadwaita::ApplicationWindow,
        settings: &gio::Settings,
        app_model: &AppModel,
        cookie: &std::cell::Cell<Option<u32>>,
    ) {
        let enabled = settings.boolean(INHIBIT_SUSPEND_KEY);
        let playing = app_model.get_state().playback.is_playing();
        Self::apply(window, cookie, should_inhibit(enabled, playing));
    }

    /// Acquire or release the inhibitor to match `desired`, guarding against
    /// double-acquire and double-release.
    fn apply(
        window: &libadwaita::ApplicationWindow,
        cookie: &std::cell::Cell<Option<u32>>,
        desired: bool,
    ) {
        let held = cookie.get().is_some();
        if desired == held {
            return;
        }

        let Some(app) = window.application() else {
            return;
        };

        if desired {
            let new_cookie = app.inhibit(
                Some(window),
                gtk::ApplicationInhibitFlags::SUSPEND,
                // Translators: Reason shown to the session manager for keeping
                // the device awake while music plays.
                Some(&gettext("Playing music")),
            );
            if new_cookie != 0 {
                cookie.set(Some(new_cookie));
            } else {
                log::warn!("Failed to acquire suspend inhibitor");
            }
        } else if let Some(old_cookie) = cookie.take() {
            app.uninhibit(old_cookie);
        }
    }
}

impl EventListener for SuspendInhibitor {
    fn on_event(&mut self, event: &AppEvent) {
        if let AppEvent::PlaybackEvent(_) = event {
            self.refresh();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::should_inhibit;

    #[test]
    fn inhibits_only_when_enabled_and_playing() {
        assert!(should_inhibit(true, true));
    }

    #[test]
    fn does_not_inhibit_when_paused_or_stopped() {
        assert!(!should_inhibit(true, false));
    }

    #[test]
    fn does_not_inhibit_when_disabled_while_playing() {
        assert!(!should_inhibit(false, true));
    }

    #[test]
    fn does_not_inhibit_when_disabled_and_not_playing() {
        assert!(!should_inhibit(false, false));
    }
}
