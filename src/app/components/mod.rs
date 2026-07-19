#[macro_export]
macro_rules! resource {
    ($resource:expr) => {
        concat!("/dev/diegovsky/Riff", $resource)
    };
}

use gettextrs::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::future::Future;

use crate::api::SpotifyApiError;
use crate::app::{ActionDispatcher, AppAction, AppEvent};

mod pages;
pub use pages::*;

mod widgets;
pub use widgets::*;

mod shell;
pub use shell::*;

mod player_notifier;
pub use player_notifier::PlayerNotifier;

mod constants;
pub use constants::*;

pub mod utils;

pub mod labels;

// without this the builder doesn't seen to know about the custom widgets
pub fn expose_custom_widgets() {
    shell::playback::expose_widgets();
    widgets::selection::expose_widgets();
    shell::headerbar::expose_widgets();
    shell::device_selector::expose_widgets();
    widgets::details_page::expose_widgets();
}

impl dyn ActionDispatcher {
    fn call_spotify_and_dispatch<F, C>(&self, call: C)
    where
        C: 'static + Send + Clone + FnOnce() -> F,
        F: Send + Future<Output = Result<AppAction, SpotifyApiError>>,
    {
        self.call_spotify_and_dispatch_many(move || async { call().await.map(|a| vec![a]) })
    }

    fn call_spotify_and_dispatch_many<F, C>(&self, call: C)
    where
        C: 'static + Send + Clone + FnOnce() -> F,
        F: Send + Future<Output = Result<Vec<AppAction>, SpotifyApiError>>,
    {
        self.dispatch_many_async(Box::pin(async move {
            let first_call = call.clone();
            let result = first_call().await;
            match result {
                Ok(actions) => actions,
                Err(SpotifyApiError::NoToken) => vec![],
                Err(SpotifyApiError::InvalidToken) => call().await.unwrap_or_else(|_| Vec::new()),
                Err(SpotifyApiError::TooManyRequests) => {
                    error!("Spotify API error: rate limited");
                    vec![AppAction::ShowNotification(gettext(
                        // translators: This notification is shown when Spotify throttles requests.
                        "Rate limited by Spotify. Please wait a moment and try again.",
                    ))]
                }
                Err(err) => {
                    error!("Spotify API error: {}", err);
                    vec![AppAction::ShowNotification(gettext(
                        // translators: This notification is the default message for unhandled errors. Logs refer to console output.
                        "An error occured. Check logs for details!",
                    ))]
                }
            }
        }))
    }
}

thread_local!(static CSS_ADDED: RefCell<HashSet<&'static str>> = RefCell::new(HashSet::new()));

pub fn display_add_css_provider(resource: &'static str) {
    CSS_ADDED.with(|set| {
        if set.borrow().contains(resource) {
            return;
        }

        set.borrow_mut().insert(resource);

        let provider = gtk::CssProvider::new();
        provider.load_from_resource(resource);

        gtk::style_context_add_provider_for_display(
            &gdk::Display::default().unwrap(),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}

pub trait EventListener {
    fn on_event(&mut self, _: &AppEvent) {}
}

pub trait Component {
    fn get_root_widget(&self) -> &gtk::Widget;

    fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
        None
    }

    fn broadcast_event(&mut self, event: &AppEvent) {
        if let Some(children) = self.get_children() {
            for child in children.iter_mut() {
                child.on_event(event);
            }
        }
    }
}

pub trait ListenerComponent: Component + EventListener {}
impl<T> ListenerComponent for T where T: Component + EventListener {}
