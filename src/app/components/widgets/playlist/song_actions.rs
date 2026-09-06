use gio::SimpleAction;

use crate::app::models::Track;
use crate::app::state::{AppAction, PlaybackAction};
use crate::app::ActionDispatcher;

/// Context-menu actions that can be built for a single track.
pub trait SongActions {
    fn make_queue_action(
        &self,
        dispatcher: Box<dyn ActionDispatcher>,
        name: Option<&str>,
    ) -> SimpleAction;

    fn make_dequeue_action(
        &self,
        dispatcher: Box<dyn ActionDispatcher>,
        name: Option<&str>,
    ) -> SimpleAction;

    fn make_link_action(&self, name: Option<&str>) -> SimpleAction;

    fn make_album_action(
        &self,
        dispatcher: Box<dyn ActionDispatcher>,
        name: Option<&str>,
    ) -> SimpleAction;

    fn make_artist_actions(
        &self,
        dispatcher: Box<dyn ActionDispatcher>,
        prefix: Option<&str>,
    ) -> Vec<SimpleAction>;
}

impl SongActions for Track {
    fn make_queue_action(
        &self,
        dispatcher: Box<dyn ActionDispatcher>,
        name: Option<&str>,
    ) -> SimpleAction {
        let queue = SimpleAction::new(name.unwrap_or("queue"), None);
        let song = self.clone();
        queue.connect_activate(move |_, _| {
            dispatcher.dispatch(PlaybackAction::Queue(vec![song.clone()]).into());
        });
        queue
    }

    fn make_dequeue_action(
        &self,
        dispatcher: Box<dyn ActionDispatcher>,
        name: Option<&str>,
    ) -> SimpleAction {
        let dequeue = SimpleAction::new(name.unwrap_or("dequeue"), None);
        let track_id = self.rri.id.clone();
        dequeue.connect_activate(move |_, _| {
            dispatcher.dispatch(PlaybackAction::Dequeue(track_id.clone()).into());
        });
        dequeue
    }

    fn make_link_action(&self, name: Option<&str>) -> SimpleAction {
        let track_id = self.rri.id.clone();
        let copy_link = SimpleAction::new(name.unwrap_or("copy_link"), None);
        copy_link.connect_activate(move |_, _| {
            let link = format!("https://open.spotify.com/track/{track_id}");
            crate::app::components::copy_link_to_clipboard(&link);
        });
        copy_link
    }

    fn make_album_action(
        &self,
        dispatcher: Box<dyn ActionDispatcher>,
        name: Option<&str>,
    ) -> SimpleAction {
        let album_id = self
            .album
            .as_ref()
            .map(|a| a.rri.id.clone())
            .unwrap_or_default();
        let view_album = SimpleAction::new(name.unwrap_or("view_album"), None);
        view_album.connect_activate(move |_, _| {
            dispatcher.dispatch(AppAction::ViewAlbum(album_id.clone()));
        });
        view_album
    }

    fn make_artist_actions(
        &self,
        dispatcher: Box<dyn ActionDispatcher>,
        prefix: Option<&str>,
    ) -> Vec<SimpleAction> {
        self.artists
            .iter()
            .map(|artist| {
                let id = artist.rri.id.clone();
                let view_artist = SimpleAction::new(
                    &format!("{}_{}", prefix.unwrap_or("view_artist"), &id),
                    None,
                );
                let dispatcher = dispatcher.box_clone();
                view_artist.connect_activate(move |_, _| {
                    dispatcher.dispatch(AppAction::ViewArtist(id.clone()));
                });
                view_artist
            })
            .collect()
    }
}
