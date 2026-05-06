use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

use crate::app::components::display_add_css_provider;
use crate::app::components::utils::{format_duration, Clock, Debouncer};
use crate::app::loader::ImageLoader;
use crate::app::models::RepeatMode;
use crate::app::Worker;

use super::playback_controls::PlaybackControlsWidget;
use super::playback_info::PlaybackInfoWidget;
use super::playback_info_mobile::PlaybackInfoMobileWidget;

mod imp {

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/playback_widget.ui")]
    pub struct PlaybackWidget {
        #[template_child]
        pub controls: TemplateChild<PlaybackControlsWidget>,

        #[template_child]
        pub mobile_controls: TemplateChild<PlaybackControlsWidget>,

        #[template_child]
        pub now_playing: TemplateChild<PlaybackInfoWidget>,

        #[template_child]
        pub mobile_now_playing: TemplateChild<PlaybackInfoMobileWidget>,

        #[template_child]
        pub seek_bar: TemplateChild<gtk::Scale>,

        #[template_child]
        pub seek_overlay: TemplateChild<gtk::Overlay>,

        #[template_child]
        pub track_position: TemplateChild<gtk::Label>,

        #[template_child]
        pub track_duration: TemplateChild<gtk::Label>,

        #[template_child]
        pub volume_slider: TemplateChild<gtk::Scale>,

        pub clock: Clock,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlaybackWidget {
        const NAME: &'static str = "PlaybackWidget";
        type Type = super::PlaybackWidget;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PlaybackWidget {
        fn constructed(&self) {
            self.parent_constructed();
            self.now_playing.set_info_visible(true);
            display_add_css_provider(resource!("/components/playback.css"));

            let track_position = self.track_position.clone();
            let track_duration = self.track_duration.clone();
            let motion = gtk::EventControllerMotion::new();
            motion.connect_enter(clone!(
                #[weak]
                track_position,
                #[weak]
                track_duration,
                move |_, _, _| {
                    track_position.set_visible(true);
                    track_duration.set_visible(true);
                }
            ));
            motion.connect_leave(clone!(
                #[weak]
                track_position,
                #[weak]
                track_duration,
                move |_| {
                    track_position.set_visible(false);
                    track_duration.set_visible(false);
                }
            ));
            self.seek_overlay.add_controller(motion);
        }
    }

    impl WidgetImpl for PlaybackWidget {}
    impl BoxImpl for PlaybackWidget {}
}

glib::wrapper! {
    pub struct PlaybackWidget(ObjectSubclass<imp::PlaybackWidget>) @extends gtk::Widget, gtk::Box;
}

impl PlaybackWidget {
    pub fn set_title_and_artist(&self, title: &str, artist: &str) {
        let widget = self.imp();
        widget.now_playing.set_visible(true);
        widget.now_playing.set_title_and_artist(title, artist);
        widget.mobile_now_playing.set_visible(true);
        widget.mobile_now_playing.set_title_and_artist(title, artist);
    }

    pub fn reset_info(&self) {
        let widget = self.imp();
        widget.now_playing.set_visible(false);
        widget.now_playing.reset_info();
        widget.mobile_now_playing.set_visible(false);
        widget.mobile_now_playing.reset_info();
        self.set_song_duration(None);
    }

    fn set_artwork(&self, image: &gdk_pixbuf::Pixbuf) {
        let widget = self.imp();
        widget.now_playing.set_artwork(image);
    }

    pub fn set_artwork_from_url(&self, url: String, worker: &Worker) {
        let weak_self = self.downgrade();
        worker.send_local_task(async move {
            let loader = ImageLoader::new();
            let result = loader.load_remote(&url, "jpg", 48, 48).await;
            if let (Some(ref _self), Some(ref result)) = (weak_self.upgrade(), result) {
                _self.set_artwork(result);
            }
        });
    }

    pub fn set_song_duration(&self, duration: Option<f64>) {
        let widget = self.imp();
        let class = "seek-bar--active";
        if let Some(duration) = duration {
            self.add_css_class(class);
            widget.seek_bar.set_range(0.0, duration);
            widget.seek_bar.set_value(0.0);
            self.update_track_time(0.0, duration);
        } else {
            self.remove_css_class(class);
            widget.seek_bar.set_range(0.0, 0.0);
        }
    }

    pub fn set_seek_position(&self, pos: f64) {
        let widget = self.imp();
        widget.seek_bar.set_value(pos);
        let duration = widget.seek_bar.adjustment().upper();
        self.update_track_time(pos, duration);
    }

    fn update_track_time(&self, pos: f64, duration: f64) {
        let widget = self.imp();
        widget.track_position.set_text(&format_duration(pos));
        widget.track_duration.set_text(&format_duration(duration));
    }

    pub fn increment_seek_position(&self) {
        let value = self.imp().seek_bar.value() + 1_000.0;
        self.set_seek_position(value);
    }

    pub fn connect_now_playing_clicked<F>(&self, f: F)
    where
        F: Fn() + Clone + 'static,
    {
        let widget = self.imp();
        let f_clone = f.clone();
        widget.now_playing.connect_clicked(move |_| f_clone());
    }

    pub fn connect_seek<Seek>(&self, seek: Seek)
    where
        Seek: Fn(u32) + Clone + 'static,
    {
        let debouncer = Debouncer::new();
        let widget = self.imp();
        widget.seek_bar.set_increments(5_000.0, 10_000.0);
        widget.seek_bar.connect_change_value(clone!(
            #[weak(rename_to = _self)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, _, requested| {
                let duration = _self.imp().seek_bar.adjustment().upper();
                _self.update_track_time(requested, duration);
                let seek = seek.clone();
                debouncer.debounce(200, move || seek(requested as u32));
                glib::Propagation::Proceed
            }
        ));
    }

    pub fn set_playing(&self, is_playing: bool) {
        let widget = self.imp();
        widget.controls.set_playing(is_playing);
        widget.mobile_controls.set_playing(is_playing);
        if is_playing {
            widget.clock.start(clone!(
                #[weak(rename_to = _self)]
                self,
                move || _self.increment_seek_position()
            ));
        } else {
            widget.clock.stop();
        }
    }

    pub fn set_repeat_mode(&self, mode: RepeatMode) {
        let widget = self.imp();
        widget.controls.set_repeat_mode(mode);
        widget.mobile_controls.set_repeat_mode(mode);
    }

    pub fn set_shuffled(&self, shuffled: bool) {
        let widget = self.imp();
        widget.controls.set_shuffled(shuffled);
        widget.mobile_controls.set_shuffled(shuffled);
    }

    pub fn set_seekbar_visible(&self, visible: bool) {
        let widget = self.imp();
        widget.seek_bar.set_visible(visible);
    }

    pub fn set_volume(&self, value: f64) {
        let widget = self.imp();
        widget.volume_slider.set_value(value)
    }

    pub fn connect_play_pause<F>(&self, f: F)
    where
        F: Fn() + Clone + 'static,
    {
        let widget = self.imp();
        widget.controls.connect_play_pause(f.clone());
        widget.mobile_controls.connect_play_pause(f);
    }

    pub fn connect_prev<F>(&self, f: F)
    where
        F: Fn() + Clone + 'static,
    {
        let widget = self.imp();
        widget.controls.connect_prev(f.clone());
        widget.mobile_controls.connect_prev(f);
    }

    pub fn connect_next<F>(&self, f: F)
    where
        F: Fn() + Clone + 'static,
    {
        let widget = self.imp();
        widget.controls.connect_next(f.clone());
        widget.mobile_controls.connect_next(f);
    }

    pub fn connect_shuffle<F>(&self, f: F)
    where
        F: Fn() + Clone + 'static,
    {
        let widget = self.imp();
        widget.controls.connect_shuffle(f.clone());
        widget.mobile_controls.connect_shuffle(f);
    }

    pub fn connect_repeat<F>(&self, f: F)
    where
        F: Fn() + Clone + 'static,
    {
        let widget = self.imp();
        widget.controls.connect_repeat(f.clone());
        widget.mobile_controls.connect_repeat(f);
    }

    pub fn connect_volume_changed<F>(&self, f: F)
    where
        F: Fn(f64) + Clone + 'static,
    {
        let widget = self.imp();
        widget
            .volume_slider
            .connect_value_changed(move |scale| f(scale.value()));
    }
}
