use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use riff_api::models::{Page, Track};
use riff_api::{ApiService, DomainError, Load};

use crate::app::components::{dispatch_api_read, EventListener};
use crate::app::models::{PageRequest, SongsSource};
use crate::app::state::{PlaybackAction, PlaybackEvent};
use crate::app::{AppAction, AppEvent, AppModel, Dispatcher};
use crate::play_queue::CONTEXT_PAGE_SIZE;

// An unanswered request is retried after this
const RETRY_AFTER: Duration = Duration::from_secs(10);

async fn get_page(
    api: &ApiService,
    source: &SongsSource,
    offset: usize,
    limit: usize,
    load: Load,
) -> Result<Page<Track>, DomainError> {
    match source {
        SongsSource::Playlist(id) => api.get_playlist_tracks(id, offset, limit, load).await,
        SongsSource::Album(id) => api.get_album_tracks(id, offset, limit, load).await,
        SongsSource::SavedTracks => api.get_saved_tracks(offset, limit, load).await,
        SongsSource::Artist(_) | SongsSource::Search(_) => {
            unreachable!("{:?} isn't paginated", source)
        }
    }
}

pub fn queue_source_tracks(app_model: &AppModel, dispatcher: &Dispatcher, source: SongsSource) {
    let api = app_model.api();
    dispatch_api_read(dispatcher, move |tag| async move {
        let mut tracks = vec![];
        loop {
            let page = get_page(&api, &source, tracks.len(), CONTEXT_PAGE_SIZE, tag).await?;
            let len = page.items.len();
            tracks.extend(page.items);
            if len < CONTEXT_PAGE_SIZE || page.total.is_some_and(|t| tracks.len() >= t) {
                break;
            }
        }
        tracks.retain(|t| t.playable);
        Ok(AppAction::QueueTracks { tracks })
    });
}

pub fn fetch_queue_page(
    app_model: &AppModel,
    dispatcher: &Dispatcher,
    source: SongsSource,
    request: PageRequest,
) {
    let api = app_model.api();
    let PageRequest { offset, batch_size } = request;
    debug!("loading queue page source={source:?} offset={offset} size={batch_size}");
    dispatch_api_read(dispatcher, move |tag| async move {
        let page = get_page(&api, &source, offset, batch_size, tag).await?;
        Ok(PlaybackAction::LoadPagedSongs(source, page).into())
    });
}

pub struct QueueLoader {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    last_request: RefCell<Option<(SongsSource, usize, Instant)>>,
}

impl QueueLoader {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        Self {
            app_model,
            dispatcher,
            last_request: RefCell::new(None),
        }
    }

    fn load_next_page(&self) {
        let query = self.app_model.get_state().playback.next_query(false);
        let Some((source, request)) = query else {
            return;
        };
        let pending = self
            .last_request
            .borrow()
            .as_ref()
            .is_some_and(|(s, offset, at)| {
                *s == source && *offset == request.offset && at.elapsed() < RETRY_AFTER
            });
        if pending {
            return;
        }
        self.last_request
            .replace(Some((source.clone(), request.offset, Instant::now())));
        fetch_queue_page(&self.app_model, &self.dispatcher, source, request);
    }
}

impl EventListener for QueueLoader {
    fn on_event(&mut self, event: &AppEvent) {
        // Not on PlaylistChanged, which loading a page sends
        if let AppEvent::PlaybackEvent(
            PlaybackEvent::TrackChanged(_)
            | PlaybackEvent::SourceChanged
            | PlaybackEvent::RepeatModeChanged(_)
            | PlaybackEvent::ShuffleChanged(_),
        ) = event
        {
            self.load_next_page();
        }
    }
}
