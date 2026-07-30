use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita::prelude::*;

use crate::app::components::{is_app_copied_link, EventListener};
use crate::app::state::SpotifyLink;
use crate::app::{ActionDispatcher, AppAction, AppModel};

/// Watches the system clipboard and, when the Riff window is focused, offers to
/// open a Spotify link the user copied from somewhere other than Riff.
pub struct ClipboardImport {
    // Kept alive for the lifetime of the component; the window signal holds a
    // weak reference to it.
    _watcher: Rc<ClipboardWatcher>,
}

impl ClipboardImport {
    pub fn new(
        window: libadwaita::ApplicationWindow,
        dispatcher: Box<dyn ActionDispatcher>,
        app_model: Rc<AppModel>,
    ) -> Self {
        let watcher = Rc::new(ClipboardWatcher {
            window: window.clone(),
            dispatcher,
            app_model,
            dismissed: RefCell::new(None),
            dialog_open: Cell::new(false),
        });

        // Check the clipboard whenever the window gains focus. A weak reference
        // avoids a reference cycle between the window and the watcher.
        let watcher_weak = Rc::downgrade(&watcher);
        window.connect_is_active_notify(move |window| {
            if !window.is_active() {
                return;
            }
            if let Some(watcher) = watcher_weak.upgrade() {
                watcher.check_clipboard();
            }
        });

        Self { _watcher: watcher }
    }
}

impl EventListener for ClipboardImport {}

struct ClipboardWatcher {
    window: libadwaita::ApplicationWindow,
    dispatcher: Box<dyn ActionDispatcher>,
    app_model: Rc<AppModel>,
    // The last link the user chose not to open (or already acted on). Prevents
    // re-prompting for the same link every time the window regains focus.
    dismissed: RefCell<Option<String>>,
    dialog_open: Cell<bool>,
}

impl ClipboardWatcher {
    fn check_clipboard(self: &Rc<Self>) {
        if self.dialog_open.get() {
            return;
        }

        let clipboard = self.window.clipboard();
        let this = Rc::clone(self);
        clipboard.read_text_async(gio::Cancellable::NONE, move |result| {
            if let Ok(Some(text)) = result {
                this.handle_text(text.to_string());
            }
        });
    }

    fn handle_text(self: &Rc<Self>, text: String) {
        let dismissed = self.dismissed.borrow().clone();
        let decision = decide(&text, is_app_copied_link(&text), dismissed.as_deref());
        if let Some(link) = decision {
            debug!("clipboard contains a Spotify {} link", link.kind_label());
            self.show_dialog(text, link);
        }
    }

    fn show_dialog(self: &Rc<Self>, text: String, link: SpotifyLink) {
        if self.dialog_open.replace(true) {
            return;
        }

        let heading = gettext("Open Spotify link?");
        let body = match &link {
            // translators: shown when an album link is found in the clipboard.
            SpotifyLink::Album(_) => {
                gettext("There’s a Spotify album link in your clipboard. Open it in Riff?")
            }
            // translators: shown when an artist link is found in the clipboard.
            SpotifyLink::Artist(_) => {
                gettext("There’s a Spotify artist link in your clipboard. Open it in Riff?")
            }
            // translators: shown when a playlist link is found in the clipboard.
            SpotifyLink::Playlist(_) => {
                gettext("There’s a Spotify playlist link in your clipboard. Open it in Riff?")
            }
            // translators: shown when a user profile link is found in the clipboard.
            SpotifyLink::User(_) => {
                gettext("There’s a Spotify profile link in your clipboard. Open it in Riff?")
            }
            // translators: shown when a track link is found in the clipboard.
            SpotifyLink::Track(_) => {
                gettext("There’s a Spotify track link in your clipboard. Open it in Riff?")
            }
        };

        let dialog = libadwaita::AlertDialog::new(Some(&heading), Some(&body));
        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("open", &gettext("Open"));
        dialog.set_response_appearance("open", libadwaita::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("open"));
        dialog.set_close_response("cancel");

        let this = Rc::clone(self);
        dialog.choose(&self.window, gio::Cancellable::NONE, move |response| {
            this.dialog_open.set(false);
            // Regardless of the choice, remember this link so we don't
            // prompt for it again until the clipboard changes.
            this.dismissed.replace(Some(text.trim().to_string()));
            if response.as_str() == "open" {
                this.open_link(link);
            }
        });
    }

    fn open_link(&self, link: SpotifyLink) {
        match link {
            SpotifyLink::Album(id) => self.dispatcher.dispatch(AppAction::ViewAlbum(id)),
            SpotifyLink::Artist(id) => self.dispatcher.dispatch(AppAction::ViewArtist(id)),
            SpotifyLink::Playlist(id) => self.dispatcher.dispatch(AppAction::ViewPlaylist(id)),
            SpotifyLink::User(id) => self.dispatcher.dispatch(AppAction::ViewUser(id)),
            SpotifyLink::Track(id) => {
                // No track detail screen exists: resolve the track's album and
                // open that instead.
                let api = self.app_model.get_spotify();
                self.dispatcher
                    .call_spotify_and_dispatch(move || async move {
                        api.get_track(&id)
                            .await
                            .map(|song| AppAction::ViewAlbum(song.album.id))
                    });
            }
        }
    }
}

/// Pure decision logic: given the clipboard text, whether it matches a link
/// Riff itself copied, and the last dismissed link, return the link to prompt
/// for (if any).
fn decide(text: &str, is_app_copied: bool, dismissed: Option<&str>) -> Option<SpotifyLink> {
    let link = SpotifyLink::parse(text)?;
    if is_app_copied {
        return None;
    }
    if dismissed == Some(text.trim()) {
        return None;
    }
    Some(link)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decide_prompts_for_external_link() {
        let text = "https://open.spotify.com/album/abc";
        assert_eq!(
            decide(text, false, None),
            Some(SpotifyLink::Album("abc".to_string()))
        );
    }

    #[test]
    fn test_decide_skips_app_copied_link() {
        let text = "https://open.spotify.com/track/abc";
        assert_eq!(decide(text, true, None), None);
    }

    #[test]
    fn test_decide_skips_dismissed_link() {
        let text = "https://open.spotify.com/track/abc";
        assert_eq!(decide(text, false, Some(text)), None);
    }

    #[test]
    fn test_decide_skips_non_spotify_text() {
        assert_eq!(decide("just some text", false, None), None);
        assert_eq!(decide("https://example.com", false, None), None);
    }

    #[test]
    fn test_decide_prompts_when_dismissed_is_different() {
        let text = "https://open.spotify.com/playlist/new";
        assert_eq!(
            decide(text, false, Some("https://open.spotify.com/playlist/old")),
            Some(SpotifyLink::Playlist("new".to_string()))
        );
    }
}
