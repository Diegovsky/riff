use crate::app::components::EventListener;
use crate::app::AppEvent;
use gdk::prelude::ToVariant;
use gettextrs::*;
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct Notification {
    toast_overlay: libadwaita::ToastOverlay,
    // Toasts that are currently showing (or queued), keyed by a dedup key.
    // Every toast goes through this map so we never stack a duplicate while one
    // with the same key is up, and so a specific toast can be dismissed later
    // (e.g. the connection banner on reconnect). Entries clear themselves when
    // the toast is dismissed, however it goes away.
    active: Rc<RefCell<HashMap<String, libadwaita::Toast>>>,
}

impl Notification {
    pub fn new(toast_overlay: libadwaita::ToastOverlay) -> Self {
        Self {
            toast_overlay,
            active: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    // Adds a toast, deduplicated by `key`. If a toast with the same key is
    // already present this is a no-op, so repeated events never stack toasts.
    fn add(&self, key: String, toast: libadwaita::Toast) {
        if self.active.borrow().contains_key(&key) {
            return;
        }

        // Drop the entry once the toast disappears (timeout, manual close, or a
        // programmatic dismiss), so the same key can be shown again afterwards.
        let active = self.active.clone();
        let dismiss_key = key.clone();
        toast.connect_dismissed(move |_| {
            active.borrow_mut().remove(&dismiss_key);
        });

        self.active.borrow_mut().insert(key, toast.clone());
        self.toast_overlay.add_toast(toast);
    }

    // Dismisses the toast with `key`, if one is showing. The dismissed handler
    // removes the map entry.
    fn dismiss(&self, key: &str) {
        // Clone the handle out and drop the borrow before dismissing, so the
        // dismissed handler can borrow the map mutably without a conflict.
        let toast = self.active.borrow().get(key).cloned();
        if let Some(toast) = toast {
            toast.dismiss();
        }
    }

    fn show(&self, content: &str) {
        let toast = libadwaita::Toast::builder()
            .title(content)
            .timeout(4)
            .build();
        // Dedup identical messages by their text.
        self.add(content.to_string(), toast);
    }

    fn show_playlist_created(&self, id: &str) {
        // translators: This is a notification that pop ups when a new playlist is created. It includes the name of that playlist.
        let message = gettext("New playlist created.");
        // translators: This is a label in the notification shown after creating a new playlist. If it is clicked, the new playlist will be opened.
        let label = gettext("View");
        let toast = libadwaita::Toast::builder()
            .title(message)
            .timeout(4)
            .action_name("app.open_playlist")
            .button_label(label)
            .action_target(&id.to_variant())
            .build();
        self.add(format!("playlist-created:{id}"), toast);
    }

    // Shows or hides the persistent "connection lost" toast. It has no timeout
    // so it stays up until we reconnect, and it keeps its standard close button
    // so the user can dismiss it manually.
    fn set_connection_lost(&self, lost: bool) {
        if lost {
            let content = gtk::Box::builder().spacing(8).build();
            let spinner = libadwaita::Spinner::builder()
                .width_request(18)
                .height_request(18)
                .valign(gtk::Align::Center)
                .build();
            // translators: Shown in a toast while the app has lost its network connection and is retrying.
            let label = gtk::Label::builder()
                .label(gettext("Connection lost. Trying to reconnect…"))
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            content.append(&spinner);
            content.append(&label);

            let toast = libadwaita::Toast::builder()
                .custom_title(&content)
                // 0 means the toast stays until it is dismissed.
                .timeout(0)
                // Show it ahead of any queued transient toasts.
                .priority(libadwaita::ToastPriority::High)
                .build();
            // add() dedups, so repeated "lost" events keep a single banner.
            self.add("connection-lost".to_string(), toast);
        } else {
            self.dismiss("connection-lost");
        }
    }
}

impl EventListener for Notification {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::NotificationShown(content) => self.show(content),
            AppEvent::PlaylistCreatedNotificationShown(id) => self.show_playlist_created(id),
            AppEvent::ConnectionLostChanged(lost) => self.set_connection_lost(*lost),
            _ => {}
        }
    }
}
