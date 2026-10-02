use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};

use super::easter_eggs::{EasterEgg, EGG_CHANCE};
use crate::app::components::labels;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarTrack {
    pub title: String,
    pub artist: String,
}

/// Seconds the handle (or the current track) shows between two texts
const HANDLE_SECONDS: u32 = 15;
/// Seconds a text that fits shows
const TEXT_SECONDS: u32 = 5;
/// Milliseconds a long text holds before and after scrolling
const SCROLL_PAUSE_MS: u64 = 1500;
/// Pixels a second a long text scrolls
const SCROLL_SPEED: f64 = 30.0;
/// Milliseconds before a new track's "Now Playing" text
const NEW_TRACK_DELAY_MS: u64 = 1000;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, glib::Properties)]
    #[template(resource = "/dev/diegovsky/Riff/components/queue_bar.ui")]
    #[properties(wrapper_type = super::QueueBarWidget)]
    pub struct QueueBarWidget {
        #[property(get, set)]
        pub show_track: Cell<bool>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub track_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub label: TemplateChild<gtk::Label>,
        #[template_child]
        pub scroller: TemplateChild<gtk::ScrolledWindow>,
        pub scroll: RefCell<Option<libadwaita::TimedAnimation>>,

        pub now_playing: RefCell<Option<BarTrack>>,
        pub up_next: RefCell<Option<BarTrack>>,
        pub up_next_turn: Cell<bool>,
        pub paused: Cell<bool>,
        pub generation: Cell<u64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for QueueBarWidget {
        const NAME: &'static str = "QueueBarWidget";
        type Type = super::QueueBarWidget;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for QueueBarWidget {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().show_text_later();
        }
    }

    impl WidgetImpl for QueueBarWidget {}
    impl BinImpl for QueueBarWidget {}
}

glib::wrapper! {
    pub struct QueueBarWidget(ObjectSubclass<imp::QueueBarWidget>)
        @extends gtk::Widget, libadwaita::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl QueueBarWidget {
    pub fn set_tracks(&self, now_playing: Option<BarTrack>, up_next: Option<BarTrack>) {
        let imp = self.imp();
        let new_track = now_playing.is_some() && now_playing != *imp.now_playing.borrow();
        imp.now_playing.replace(now_playing);
        imp.up_next.replace(up_next);
        self.update_track_label();
        if new_track {
            imp.up_next_turn.set(false);
            let millis = if self.show_track() {
                u64::from(HANDLE_SECONDS) * 1000
            } else {
                NEW_TRACK_DELAY_MS
            };
            self.restart(millis);
        } else if imp.now_playing.borrow().is_none() {
            self.restart(u64::from(HANDLE_SECONDS) * 1000);
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.imp().paused.set(paused);
        self.update_track_label();
    }

    fn update_track_label(&self) {
        let imp = self.imp();
        let markup = imp.now_playing.borrow().as_ref().map(|track| {
            if imp.paused.get() {
                labels::current_track_markup(&track.title, &track.artist)
            } else {
                labels::now_playing_markup(&track.title, &track.artist)
            }
        });
        imp.track_label
            .set_markup(markup.as_deref().unwrap_or_default());
    }

    fn restart(&self, millis: u64) {
        let imp = self.imp();
        imp.generation.set(imp.generation.get() + 1);
        if let Some(scroll) = imp.scroll.take() {
            scroll.pause();
        }
        imp.stack.set_visible_child_name("rest");
        self.after(millis, Self::show_text);
    }

    fn after(&self, millis: u64, f: fn(&Self)) {
        let bar = self.downgrade();
        let generation = self.imp().generation.get();
        glib::timeout_add_local_once(std::time::Duration::from_millis(millis), move || {
            if let Some(bar) = bar.upgrade() {
                if bar.imp().generation.get() == generation {
                    f(&bar);
                }
            }
        });
    }

    fn show_text_later(&self) {
        self.after(u64::from(HANDLE_SECONDS) * 1000, Self::show_text);
    }

    fn show_text(&self) {
        let imp = self.imp();
        let now_playing = imp.now_playing.borrow().clone();
        let (true, Some(now_playing)) = (self.is_mapped(), now_playing) else {
            self.show_text_later();
            return;
        };
        if self.settings().is_gtk_enable_animations() && glib::random_int_range(0, EGG_CHANCE) == 0
        {
            self.show_egg(EasterEgg::random());
            return;
        }
        let up_next = imp.up_next.borrow().clone();
        if self.show_track() {
            imp.up_next_turn.set(true);
            if up_next.is_none() {
                self.show_text_later();
                return;
            }
        }
        let markup = match up_next {
            Some(next) if imp.up_next_turn.get() => {
                labels::up_next_markup(&next.title, &next.artist)
            }
            _ if imp.paused.get() => {
                labels::current_track_markup(&now_playing.title, &now_playing.artist)
            }
            _ => labels::now_playing_markup(&now_playing.title, &now_playing.artist),
        };
        imp.up_next_turn.set(!imp.up_next_turn.get());
        imp.label.set_markup(&markup);
        imp.scroller.hadjustment().set_value(0.0);
        imp.stack.set_visible_child_name("text");

        self.after(self.slide_ms() + SCROLL_PAUSE_MS, Self::scroll_text);
    }

    fn show_egg(&self, egg: EasterEgg) {
        let imp = self.imp();
        let mut frames = egg.frames().into_iter();
        imp.label.set_markup(&frames.next().unwrap_or_default());
        imp.scroller.hadjustment().set_value(0.0);
        imp.stack.set_visible_child_name("text");

        let bar = self.downgrade();
        let generation = imp.generation.get();
        let interval = std::time::Duration::from_millis(egg.frame_ms());
        glib::timeout_add_local(interval, move || {
            let Some(bar) = bar.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if bar.imp().generation.get() != generation {
                return glib::ControlFlow::Break;
            }
            match frames.next() {
                Some(frame) => {
                    bar.imp().label.set_markup(&frame);
                    glib::ControlFlow::Continue
                }
                None => {
                    bar.after(SCROLL_PAUSE_MS, Self::show_handle);
                    glib::ControlFlow::Break
                }
            }
        });
    }

    fn scroll_text(&self) {
        let imp = self.imp();
        let adjustment = imp.scroller.hadjustment();
        let overflow = adjustment.upper() - adjustment.page_size();
        if overflow <= 0.0 {
            let shown = self.slide_ms() + SCROLL_PAUSE_MS;
            let rest = (u64::from(TEXT_SECONDS) * 1000).saturating_sub(shown);
            self.after(rest, Self::show_handle);
            return;
        }
        let duration = (overflow / SCROLL_SPEED * 1000.0) as u32;
        let target = libadwaita::PropertyAnimationTarget::new(&adjustment, "value");
        let scroll = libadwaita::TimedAnimation::new(self, 0.0, overflow, duration, target);
        scroll.set_easing(libadwaita::Easing::EaseInOutSine);
        let bar = self.downgrade();
        let generation = imp.generation.get();
        scroll.connect_done(move |_| {
            if let Some(bar) = bar.upgrade() {
                if bar.imp().generation.get() == generation {
                    bar.after(SCROLL_PAUSE_MS, Self::show_handle);
                }
            }
        });
        scroll.play();
        imp.scroll.replace(Some(scroll));
    }

    fn slide_ms(&self) -> u64 {
        u64::from(self.imp().stack.transition_duration())
    }

    fn show_handle(&self) {
        self.imp().stack.set_visible_child_name("rest");
        self.show_text_later();
    }
}
