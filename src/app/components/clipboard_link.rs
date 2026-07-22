//! Tracks the last Spotify link that Riff itself wrote to the clipboard.
//!
//! Riff has "Copy link" actions that place `https://open.spotify.com/...` URLs
//! on the clipboard. The clipboard watcher should not turn around and offer to
//! open a link the user just copied *from* Riff, so those actions record the
//! link here and the watcher checks against it.
//!
//! GTK runs entirely on the main-context thread, so a `thread_local!` is a
//! simple and correct place to keep this shared state.

use std::cell::RefCell;

use gdk::prelude::*;

thread_local! {
    static LAST_APP_COPIED_LINK: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Record a link that Riff just wrote to the clipboard.
pub fn remember_app_copied_link(link: &str) {
    let link = link.trim().to_string();
    LAST_APP_COPIED_LINK.with(|last| *last.borrow_mut() = Some(link));
}

/// Copy a link to the clipboard and record it as app-copied, so the clipboard
/// watcher won't offer to open the link the user just copied from Riff.
pub fn copy_link_to_clipboard(link: &str) {
    let clipboard = gdk::Display::default().unwrap().clipboard();
    clipboard
        .set_content(Some(&gdk::ContentProvider::for_value(&link.to_value())))
        .expect("Failed to set clipboard content");
    remember_app_copied_link(link);
}

/// Whether the given clipboard text is (still) the last link Riff copied.
pub fn is_app_copied_link(text: &str) -> bool {
    let text = text.trim();
    LAST_APP_COPIED_LINK.with(|last| last.borrow().as_deref() == Some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remember_and_match() {
        let link = "https://open.spotify.com/track/remember_and_match";
        remember_app_copied_link(link);
        assert!(is_app_copied_link(link));
        // Matching is whitespace-insensitive on the incoming text.
        assert!(is_app_copied_link(
            "  https://open.spotify.com/track/remember_and_match \n"
        ));
    }

    #[test]
    fn test_non_matching_link() {
        remember_app_copied_link("https://open.spotify.com/track/aaa_non_matching");
        assert!(!is_app_copied_link(
            "https://open.spotify.com/track/bbb_non_matching"
        ));
    }
}
