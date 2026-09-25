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

use crate::app::{load, AppAction, AppEvent, Dispatcher};
use riff_api::{DomainError, Load, LoadPriority};

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

pub mod clipboard_link;
pub use clipboard_link::{copy_link_to_clipboard, is_app_copied_link};

// without this the builder doesn't seen to know about the custom widgets
pub fn expose_custom_widgets() {
    shell::playback::expose_widgets();
    widgets::selection::expose_widgets();
    shell::headerbar::expose_widgets();
    shell::device_selector::expose_widgets();
    widgets::details_page::expose_widgets();
    shell::window::expose_widgets();
}

/// Run an api call that needs no scheduling tag (a mutation).
pub fn dispatch_api_call<F, C>(dispatcher: &Dispatcher, call: C)
where
    C: 'static + Send + Clone + FnOnce() -> F,
    F: Send + Future<Output = Result<AppAction, DomainError>>,
{
    dispatch_api_call_many(dispatcher, move || async { call().await.map(|a| vec![a]) })
}

/// As [`dispatch_api_call`], for several actions.
pub fn dispatch_api_call_many<F, C>(dispatcher: &Dispatcher, call: C)
where
    C: 'static + Send + Clone + FnOnce() -> F,
    F: 'static + Send + Future<Output = Result<Vec<AppAction>, DomainError>>,
{
    spawn_api_call(dispatcher, call)
}

/// Run an api read for the current page, tagged [`LoadPriority::Visible`].
///
/// Built here rather than inside the closure, so a read issued just before a
/// navigation cannot pick up the new epoch and compete with the page the user
/// moved to.
pub fn dispatch_api_read<F, C>(dispatcher: &Dispatcher, call: C)
where
    C: 'static + Send + Clone + FnOnce(Load) -> F,
    F: Send + Future<Output = Result<AppAction, DomainError>>,
{
    dispatch_api_read_many(dispatcher, move |load| async move {
        call(load).await.map(|a| vec![a])
    })
}

/// As [`dispatch_api_read`], for several actions.
pub fn dispatch_api_read_many<F, C>(dispatcher: &Dispatcher, call: C)
where
    C: 'static + Send + Clone + FnOnce(Load) -> F,
    F: 'static + Send + Future<Output = Result<Vec<AppAction>, DomainError>>,
{
    let load = load::at(LoadPriority::Visible);
    spawn_api_call(dispatcher, move || call(load))
}

pub fn dispatch_api_read_with_fallback<F, C, OnFail>(
    dispatcher: &Dispatcher,
    call: C,
    on_fail: OnFail,
) where
    C: 'static + Send + Clone + FnOnce(Load) -> F,
    F: 'static + Send + Future<Output = Result<AppAction, DomainError>>,
    OnFail: 'static + Send + FnOnce() -> AppAction,
{
    let load = load::at(LoadPriority::Visible);
    let dispatcher = dispatcher.clone();
    tokio::spawn(async move {
        let (mut actions, succeeded) = resolve_api_call(move || {
            let call = call.clone();
            async move { call(load).await.map(|a| vec![a]) }
        })
        .await;
        if !succeeded {
            actions.push(on_fail());
        }
        dispatcher.dispatch_many(actions);
    });
}

fn spawn_api_call<F, C>(dispatcher: &Dispatcher, call: C)
where
    C: 'static + Send + Clone + FnOnce() -> F,
    F: 'static + Send + Future<Output = Result<Vec<AppAction>, DomainError>>,
{
    let dispatcher = dispatcher.clone();
    tokio::spawn(async move {
        dispatcher.dispatch_many(resolve_api_call(call).await.0);
    });
}

/// Resolves an api call to the actions it produces, alongside whether the
/// underlying call ultimately succeeded (after any internal auth retry).
async fn resolve_api_call<F, C>(call: C) -> (Vec<AppAction>, bool)
where
    C: Clone + FnOnce() -> F,
    F: Future<Output = Result<Vec<AppAction>, DomainError>>,
{
    let first_call = call.clone();
    let result = first_call().await;
    match result {
        Ok(actions) => (actions, true),
        Err(DomainError::NoToken) => (vec![], false),
        Err(DomainError::Shed) => (vec![], false),
        Err(DomainError::AuthExpired) => {
            let retried = call().await;
            let ok = retried.is_ok();
            (retried.unwrap_or_else(|_| Vec::new()), ok)
        }
        Err(DomainError::RateLimited { .. }) => {
            error!("Spotify API error: rate limited");
            (
                vec![AppAction::ShowNotification(gettext(
                    // translators: This notification is shown when Spotify throttles requests.
                    "Rate limited by Spotify. Please wait a moment and try again.",
                ))],
                false,
            )
        }
        Err(err) => {
            // "Simulate Offline" surfaces as a network error in debug builds;
            // the connection-lost banner already covers it, so skip the toast.
            #[cfg(debug_assertions)]
            if riff_api::is_simulate_offline() {
                return (vec![], false);
            }
            error!("Spotify API error: {}", err);
            (
                vec![AppAction::ShowNotification(gettext(
                    // translators: This notification is the default message for unhandled errors. Logs refer to console output.
                    "An error occured. Check logs for details!",
                ))],
                false,
            )
        }
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
