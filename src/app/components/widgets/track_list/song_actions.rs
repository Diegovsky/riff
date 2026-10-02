use gio::SimpleAction;

use crate::app::models::Track;
use crate::app::state::{AppAction, EntryKey, PlaybackAction};
use crate::app::Dispatcher;

pub trait SongActions {
    fn make_queue_actions(&self, dispatcher: Dispatcher) -> Vec<SimpleAction>;
    fn make_queue_entry_actions(&self, key: EntryKey, dispatcher: Dispatcher) -> Vec<SimpleAction>;
    fn make_link_action(&self) -> SimpleAction;
    fn make_album_action(&self, dispatcher: Dispatcher) -> SimpleAction;
    fn make_artist_actions(&self, dispatcher: Dispatcher) -> Vec<SimpleAction>;
}

fn action(name: &str, activate: impl Fn() + 'static) -> SimpleAction {
    let action = SimpleAction::new(name, None);
    action.connect_activate(move |_, _| activate());
    action
}

impl SongActions for Track {
    fn make_queue_actions(&self, dispatcher: Dispatcher) -> Vec<SimpleAction> {
        let song = self.clone();
        vec![action("queue", move || {
            dispatcher.dispatch(AppAction::QueueTracks {
                tracks: vec![song.clone()],
            });
        })]
    }

    fn make_queue_entry_actions(&self, key: EntryKey, dispatcher: Dispatcher) -> Vec<SimpleAction> {
        vec![action("dequeue", move || {
            dispatcher.dispatch(PlaybackAction::RemoveEntries(vec![key]).into())
        })]
    }

    fn make_link_action(&self) -> SimpleAction {
        let link = format!("https://open.spotify.com/track/{}", self.rri.id);
        action("copy_link", move || {
            crate::app::components::copy_link_to_clipboard(&link);
        })
    }

    fn make_album_action(&self, dispatcher: Dispatcher) -> SimpleAction {
        let id = self
            .album
            .as_ref()
            .map(|a| a.rri.id.clone())
            .unwrap_or_default();
        action("view_album", move || {
            dispatcher.dispatch(AppAction::ViewAlbum(id.clone()));
        })
    }

    fn make_artist_actions(&self, dispatcher: Dispatcher) -> Vec<SimpleAction> {
        self.artists
            .iter()
            .map(|artist| {
                let id = artist.rri.id.clone();
                let dispatcher = dispatcher.clone();
                action(&format!("view_artist_{id}"), move || {
                    dispatcher.dispatch(AppAction::ViewArtist(id.clone()));
                })
            })
            .collect()
    }
}
