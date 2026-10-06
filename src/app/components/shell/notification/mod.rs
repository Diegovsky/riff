use crate::app::components::EventListener;
use crate::app::AppEvent;
use gdk::prelude::ToVariant;
use gettextrs::*;
use gtk::prelude::*;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const CONNECTION_LOST_KEY: &str = "connection-lost";

type ActiveToasts = Rc<RefCell<HashMap<String, libadwaita::Toast>>>;

#[derive(Clone)]
pub struct Notification {
    toast_overlay: libadwaita::ToastOverlay,
    active: ActiveToasts,
}

impl Notification {
    pub fn new(toast_overlay: libadwaita::ToastOverlay) -> Self {
        let notification = Self {
            toast_overlay,
            active: Rc::new(RefCell::new(HashMap::new())),
        };
        if let Some(window) = notification.window() {
            let notification = notification.clone();
            window.connect_visible_dialog_notify(move |_| notification.dialog_changed());
        }
        notification
    }

    fn window(&self) -> Option<libadwaita::ApplicationWindow> {
        self.toast_overlay.root()?.downcast().ok()
    }

    fn current_overlay(&self) -> (libadwaita::ToastOverlay, Option<libadwaita::Dialog>) {
        self.window()
            .and_then(|window| window.visible_dialog())
            .and_then(|dialog| Some((find_toast_overlay(dialog.upcast_ref())?, Some(dialog))))
            .unwrap_or_else(|| (self.toast_overlay.clone(), None))
    }

    fn dialog_changed(&self) {
        let connection_lost = self.active.borrow().contains_key(CONNECTION_LOST_KEY);
        if connection_lost {
            self.set_connection_lost(false);
            self.set_connection_lost(true);
        }
    }

    fn add(&self, key: String, toast: libadwaita::Toast) {
        if self.active.borrow().contains_key(&key) {
            return;
        }

        let active = self.active.clone();
        let dismiss_key = key.clone();
        toast.connect_dismissed(move |_| {
            active.borrow_mut().remove(&dismiss_key);
        });

        let (overlay, dialog) = self.current_overlay();
        if let Some(dialog) = dialog.filter(|_| key != CONNECTION_LOST_KEY) {
            let toast = toast.clone();
            dialog.connect_closed(move |_| toast.dismiss());
        }
        self.active.borrow_mut().insert(key, toast.clone());
        overlay.add_toast(toast);
    }

    fn dismiss(&self, key: &str) {
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
                .timeout(0)
                .priority(libadwaita::ToastPriority::High)
                .build();
            self.add(CONNECTION_LOST_KEY.to_string(), toast);
        } else {
            self.dismiss(CONNECTION_LOST_KEY);
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

fn find_toast_overlay(root: &gtk::Widget) -> Option<libadwaita::ToastOverlay> {
    let mut queue = std::collections::VecDeque::from([root.clone()]);
    while let Some(widget) = queue.pop_front() {
        if let Some(overlay) = widget.downcast_ref::<libadwaita::ToastOverlay>() {
            return Some(overlay.clone());
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            queue.push_back(widget);
        }
    }
    None
}
