use std::ops::Deref;
use std::rc::Rc;

use gtk::prelude::*;

use crate::app::components::sidebar::SidebarDestination;
use crate::app::components::EventListener;
use crate::app::models::*;
use crate::app::state::{PlaybackAction, PlaybackEvent, ScreenName, SelectionEvent};
use crate::app::{ActionDispatcher, AppEvent, AppModel, AppState, BrowserAction, Worker};

use super::playback_widget::PlaybackWidget;
use super::PlaybackInfoMobileWidget;

pub struct PlaybackModel {
    app_model: Rc<AppModel>,
    dispatcher: Box<dyn ActionDispatcher>,
}

impl PlaybackModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Box<dyn ActionDispatcher>) -> Self {
        Self {
            app_model,
            dispatcher,
        }
    }

    fn state(&self) -> impl Deref<Target = AppState> + '_ {
        self.app_model.get_state()
    }

    fn go_home(&self) {
        // Reach now-playing like the sidebar does: pop to home and select its
        // now-playing sub-page, reusing the home sub-page switch path.
        self.dispatcher.dispatch_many(vec![
            BrowserAction::NavigationPopTo(ScreenName::Home).into(),
            BrowserAction::SetHomeVisiblePage(SidebarDestination::NowPlaying.id()).into(),
        ]);
    }

    fn is_playing(&self) -> bool {
        self.state().playback.is_playing()
    }

    fn is_shuffled(&self) -> bool {
        self.state().playback.is_shuffled()
    }

    fn current_song(&self) -> Option<SongDescription> {
        self.app_model.get_state().playback.current_song()
    }

    fn play_next_song(&self) {
        self.dispatcher.dispatch(PlaybackAction::Next.into());
    }

    fn play_prev_song(&self) {
        self.dispatcher.dispatch(PlaybackAction::Previous.into());
    }

    fn toggle_playback(&self) {
        self.dispatcher.dispatch(PlaybackAction::TogglePlay.into());
    }

    fn toggle_shuffle(&self) {
        self.dispatcher
            .dispatch(PlaybackAction::ToggleShuffle.into());
    }

    fn toggle_repeat(&self) {
        self.dispatcher
            .dispatch(PlaybackAction::ToggleRepeat.into());
    }

    fn seek_to(&self, position: u32) {
        self.dispatcher
            .dispatch(PlaybackAction::Seek(position).into());
    }

    fn set_volume(&self, value: f64) {
        self.dispatcher
            .dispatch(PlaybackAction::SetVolume(value).into())
    }
}

pub struct PlaybackControl {
    model: Rc<PlaybackModel>,
    widget: PlaybackWidget,
    mobile_now_playing: PlaybackInfoMobileWidget,
    worker: Worker,
}

impl PlaybackControl {
    pub fn new(
        model: PlaybackModel,
        widget: PlaybackWidget,
        mobile_now_playing: PlaybackInfoMobileWidget,
        worker: Worker,
    ) -> Self {
        let model = Rc::new(model);

        widget.connect_play_pause(clone!(
            #[weak]
            model,
            move || model.toggle_playback()
        ));
        widget.connect_next(clone!(
            #[weak]
            model,
            move || model.play_next_song()
        ));
        widget.connect_prev(clone!(
            #[weak]
            model,
            move || model.play_prev_song()
        ));
        widget.connect_shuffle(clone!(
            #[weak]
            model,
            move || model.toggle_shuffle()
        ));
        widget.connect_repeat(clone!(
            #[weak]
            model,
            move || model.toggle_repeat()
        ));
        widget.connect_seek(clone!(
            #[weak]
            model,
            move |position| model.seek_to(position)
        ));
        widget.connect_now_playing_clicked(clone!(
            #[weak]
            model,
            move || model.go_home()
        ));
        widget.connect_volume_changed(clone!(
            #[weak]
            model,
            move |value| model.set_volume(value)
        ));

        Self {
            model,
            widget,
            mobile_now_playing,
            worker,
        }
    }

    fn update_repeat(&self, mode: &RepeatMode) {
        self.widget.set_repeat_mode(*mode);
    }

    fn update_shuffled(&self) {
        self.widget.set_shuffled(self.model.is_shuffled());
    }

    fn update_playing(&self) {
        let is_playing = self.model.is_playing();
        self.widget.set_playing(is_playing);
    }

    fn update_current_info(&self) {
        if let Some(song) = self.model.current_song() {
            self.widget
                .set_title_and_artist(&song.title, &song.artists_name());
            self.mobile_now_playing
                .set_title_and_artist(&song.title, &song.artists_name());
            self.mobile_now_playing.set_visible(true);
            self.widget.set_song_duration(Some(song.duration_ms as f64));
            if let Some(url) = song.art.as_ref().and_then(|s| s.best_for_width(120)) {
                self.widget
                    .set_artwork_from_url(url.to_owned(), &self.worker);
            }
        } else {
            self.widget.reset_info();
            self.mobile_now_playing.reset_info();
            self.mobile_now_playing.set_visible(false);
        }
    }

    fn sync_seek(&self, pos: u32) {
        self.widget.set_seek_position(pos as f64);
    }

    fn set_connection_lost(&self, lost: bool) {
        // Do NOT freeze the seekbar during an outage. A dead session does not
        // stop playback: buffered audio keeps streaming from the CDN, so the
        // local clock stays in sync with what the user actually hears. Freezing
        // it here left the seekbar behind, and since no Playing event fires when
        // playback was never interrupted, it never resynced once the session
        // came back. If playback really was interrupted, the reload after
        // reconnect emits a SyncSeek that corrects the position. We only make
        // sure the clock is running again on restore, in case playback was
        // paused and resumed during the outage. The "reconnecting" indicator
        // itself is shown as a persistent toast by the Notification component.
        if !lost && self.model.is_playing() {
            self.widget.resume_seek_position();
        }
    }
}

impl EventListener for PlaybackControl {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::PlaybackEvent(PlaybackEvent::PlaybackPaused)
            | AppEvent::PlaybackEvent(PlaybackEvent::PlaybackResumed) => {
                self.update_playing();
            }
            AppEvent::PlaybackEvent(PlaybackEvent::RepeatModeChanged(mode)) => {
                self.update_repeat(mode);
            }
            AppEvent::PlaybackEvent(PlaybackEvent::ShuffleChanged(_)) => {
                self.update_shuffled();
            }
            AppEvent::PlaybackEvent(PlaybackEvent::TrackChanged(_)) => {
                self.update_playing();
                self.update_current_info();
            }
            AppEvent::PlaybackEvent(PlaybackEvent::PlaybackStopped) => {
                self.update_playing();
                self.update_current_info();
            }
            AppEvent::PlaybackEvent(PlaybackEvent::SeekSynced(pos))
            | AppEvent::PlaybackEvent(PlaybackEvent::TrackSeeked(pos)) => {
                self.sync_seek(*pos);
            }
            AppEvent::SelectionEvent(SelectionEvent::SelectionModeChanged(active)) => {
                self.widget.set_seekbar_visible(!active);
            }
            AppEvent::PlaybackEvent(PlaybackEvent::VolumeSet(value)) => {
                self.widget.set_volume(*value)
            }
            AppEvent::ConnectionLostChanged(lost) => {
                self.set_connection_lost(*lost);
            }
            _ => {}
        }
    }
}
