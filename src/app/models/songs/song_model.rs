#![allow(clippy::all)]

use gio::prelude::*;
use glib::{subclass::prelude::*, SignalHandlerId};
use std::{cell::Ref, ops::Deref};

use crate::app::components::utils::format_duration;
use crate::app::models::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueRole {
    Queued,
    Context,
    Current,
    Fixed,
}

// UI model for a song
glib::wrapper! {
    pub struct SongModel(ObjectSubclass<imp::SongModel>);
}

impl SongModel {
    pub fn new(song: Track) -> Self {
        let o: Self = glib::Object::new();
        o.imp().song.replace(Some(song));
        o
    }

    pub fn new_keyed(song: Track, key: String) -> Self {
        let o = Self::new(song);
        o.imp().key.replace(Some(key));
        o
    }

    pub fn set_queue_role(&self, role: Option<QueueRole>) {
        self.imp().queue_role.set(role);
    }

    pub fn queue_role(&self) -> Option<QueueRole> {
        self.imp().queue_role.get()
    }

    pub fn set_group(&self, group: Option<String>) {
        self.imp().group.replace(group);
    }

    pub fn group(&self) -> Option<String> {
        self.imp().group.borrow().clone()
    }

    pub fn row_key(&self) -> String {
        self.imp()
            .key
            .borrow()
            .clone()
            .unwrap_or_else(|| self.get_id())
    }

    pub fn set_playing(&self, is_playing: bool) {
        self.set_property("playing", is_playing);
    }

    pub fn set_selected(&self, is_selected: bool) {
        self.set_property("selected", is_selected);
    }

    pub fn set_liked(&self, is_liked: bool) {
        self.set_property("liked", is_liked);
    }

    pub fn set_pinned(&self, is_pinned: bool) {
        self.set_property("pinned", is_pinned);
    }

    pub fn set_explicit_filtered(&self, is_explicit_filtered: bool) {
        self.set_property("explicit-filtered", is_explicit_filtered);
    }

    pub fn get_playing(&self) -> bool {
        self.property("playing")
    }

    pub fn get_selected(&self) -> bool {
        self.property("selected")
    }

    pub fn get_liked(&self) -> bool {
        self.property("liked")
    }

    pub fn get_pinned(&self) -> bool {
        self.property("pinned")
    }

    pub fn get_id(&self) -> String {
        self.property("id")
    }

    pub fn bind_index(&self, o: &impl ObjectType, property: &str) {
        self.imp().push_binding(
            self.bind_property("index", o, property)
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build(),
        );
    }

    pub fn bind_artist(&self, o: &impl ObjectType, property: &str) {
        self.imp().push_binding(
            self.bind_property("artist", o, property)
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build(),
        );
    }

    pub fn bind_title(&self, o: &impl ObjectType, property: &str) {
        self.imp().push_binding(
            self.bind_property("title", o, property)
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build(),
        );
    }

    pub fn bind_duration(&self, o: &impl ObjectType, property: &str) {
        self.imp().push_binding(
            self.bind_property("duration", o, property)
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build(),
        );
    }

    pub fn bind_playing(&self, o: &impl ObjectType, property: &str) {
        self.imp().push_binding(
            self.bind_property("playing", o, property)
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build(),
        );
    }

    pub fn bind_selected(&self, o: &impl ObjectType, property: &str) {
        self.imp().push_binding(
            self.bind_property("selected", o, property)
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build(),
        );
    }

    pub fn bind_liked(&self, o: &impl ObjectType, property: &str) {
        self.imp().push_binding(
            self.bind_property("liked", o, property)
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build(),
        );
    }

    pub fn bind_playable(&self, o: &impl ObjectType, property: &str) {
        self.imp().push_binding(
            self.bind_property("playable", o, property)
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build(),
        );
    }

    pub fn bind_explicit_filtered(&self, o: &impl ObjectType, property: &str) {
        self.imp().push_binding(
            self.bind_property("explicit-filtered", o, property)
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build(),
        );
    }

    pub fn begin_bindings(&self, owner: usize) {
        self.unbind(owner);
        self.imp().binding_owner.set(owner);
    }

    pub fn unbind(&self, owner: usize) {
        self.imp().unbind(self, owner);
    }

    pub fn push_signal(&self, id: SignalHandlerId) {
        self.imp().push_signal(id);
    }

    pub fn description(&self) -> impl Deref<Target = Track> + '_ {
        Ref::map(self.imp().song.borrow(), |s| {
            s.as_ref().expect("song set at constructor")
        })
    }

    pub fn into_description(&self) -> Track {
        self.imp()
            .song
            .borrow()
            .as_ref()
            .cloned()
            .expect("song set at constructor")
    }
}

mod imp {

    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    // Keep track of signals and bindings targeting this song, by the row
    // they are for
    #[derive(Default)]
    struct BindingsInner {
        pub signals: Vec<SignalHandlerId>,
        pub bindings: Vec<glib::Binding>,
    }

    #[derive(Default)]
    pub struct SongModel {
        pub song: RefCell<Option<Track>>,
        pub key: RefCell<Option<String>>,
        pub queue_role: Cell<Option<QueueRole>>,
        pub group: RefCell<Option<String>>,
        pub state: Cell<SongState>,
        bindings: RefCell<HashMap<usize, BindingsInner>>,
        pub binding_owner: Cell<usize>,
    }

    impl SongModel {
        fn with_owner_bindings(&self, f: impl FnOnce(&mut BindingsInner)) {
            let mut bindings = self.bindings.borrow_mut();
            f(bindings.entry(self.binding_owner.get()).or_default());
        }

        pub fn push_signal(&self, id: SignalHandlerId) {
            self.with_owner_bindings(|b| b.signals.push(id));
        }

        pub fn push_binding(&self, binding: glib::Binding) {
            self.with_owner_bindings(|b| b.bindings.push(binding));
        }

        pub fn unbind<O: ObjectExt>(&self, o: &O, owner: usize) {
            let Some(bindings) = self.bindings.borrow_mut().remove(&owner) else {
                return;
            };
            bindings.signals.into_iter().for_each(|s| o.disconnect(s));
            bindings.bindings.into_iter().for_each(|b| b.unbind());
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SongModel {
        const NAME: &'static str = "SongModel";
        type Type = super::SongModel;
        type ParentType = glib::Object;
    }

    lazy_static! {
        static ref PROPERTIES: [glib::ParamSpec; 12] = [
            glib::ParamSpecString::builder("id").read_only().build(),
            glib::ParamSpecUInt::builder("index").read_only().build(),
            glib::ParamSpecString::builder("title").read_only().build(),
            glib::ParamSpecString::builder("artist").read_only().build(),
            glib::ParamSpecString::builder("duration")
                .read_only()
                .build(),
            // URL
            glib::ParamSpecString::builder("art").read_only().build(),
            // Can be true when playback is paused; just means this is the current song
            glib::ParamSpecBoolean::builder("playing")
                .readwrite()
                .explicit_notify()
                .build(),
            glib::ParamSpecBoolean::builder("selected")
                .readwrite()
                .explicit_notify()
                .build(),
            glib::ParamSpecBoolean::builder("liked")
                .readwrite()
                .explicit_notify()
                .build(),
            glib::ParamSpecBoolean::builder("pinned")
                .readwrite()
                .explicit_notify()
                .build(),
            glib::ParamSpecBoolean::builder("playable")
                .read_only()
                .build(),
            glib::ParamSpecBoolean::builder("explicit-filtered")
                .readwrite()
                .explicit_notify()
                .build(),
        ];
    }

    impl ObjectImpl for SongModel {
        fn properties() -> &'static [glib::ParamSpec] {
            &*PROPERTIES
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let state = self.state.get();
            let new_state = match pspec.name() {
                "playing" => SongState {
                    is_playing: value
                        .get()
                        .expect("type conformity checked by `Object::set_property`"),
                    ..state
                },
                "selected" => SongState {
                    is_selected: value
                        .get()
                        .expect("type conformity checked by `Object::set_property`"),
                    ..state
                },
                "liked" => SongState {
                    is_liked: value
                        .get()
                        .expect("type conformity checked by `Object::set_property`"),
                    ..state
                },
                "pinned" => SongState {
                    is_pinned: value
                        .get()
                        .expect("type conformity checked by `Object::set_property`"),
                    ..state
                },
                "explicit-filtered" => SongState {
                    is_explicit_filtered: value
                        .get()
                        .expect("type conformity checked by `Object::set_property`"),
                    ..state
                },
                _ => unimplemented!(),
            };

            if new_state == state {
                return;
            }
            self.state.set(new_state);
            // These properties are `explicit_notify`, so we own the notification.
            self.obj().notify_by_pspec(pspec);
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "index" => self
                    .song
                    .borrow()
                    .as_ref()
                    .expect("song set at constructor")
                    .track_number
                    .unwrap_or(1)
                    .to_value(),
                "title" => self
                    .song
                    .borrow()
                    .as_ref()
                    .expect("song set at constructor")
                    .title
                    .to_value(),
                "artist" => self
                    .song
                    .borrow()
                    .as_ref()
                    .expect("song set at constructor")
                    .artists_name()
                    .to_value(),
                "id" => self
                    .song
                    .borrow()
                    .as_ref()
                    .expect("song set at constructor")
                    .rri
                    .id
                    .to_value(),
                "duration" => self
                    .song
                    .borrow()
                    .as_ref()
                    .map(|s| format_duration(s.duration_ms.into()))
                    .expect("song set at constructor")
                    .to_value(),
                "art" => self
                    .song
                    .borrow()
                    .as_ref()
                    .expect("song set at constructor")
                    .art
                    .best_for_width(48)
                    .map(str::to_owned)
                    .to_value(),
                "playing" => self.state.get().is_playing.to_value(),
                "selected" => self.state.get().is_selected.to_value(),
                "liked" => self.state.get().is_liked.to_value(),
                "pinned" => self.state.get().is_pinned.to_value(),
                "playable" => self
                    .song
                    .borrow()
                    .as_ref()
                    .expect("song set at constructor")
                    .playable
                    .to_value(),
                "explicit-filtered" => self.state.get().is_explicit_filtered.to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::models::make_track;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn test_unbinding_a_row_leaves_the_song_bound_to_its_other_rows() {
        let song = SongModel::new(make_track("song1"));
        let (row_a, row_b) = (
            SongModel::new(make_track("a")),
            SongModel::new(make_track("b")),
        );
        let is_selected = |row: &SongModel| row.property::<bool>("selected");

        song.begin_bindings(1);
        song.bind_selected(&row_a, "selected");
        song.begin_bindings(2);
        song.bind_selected(&row_b, "selected");
        song.set_selected(true);
        assert!(is_selected(&row_a) && is_selected(&row_b));

        song.unbind(2);
        song.set_selected(false);
        assert!(!is_selected(&row_a) && is_selected(&row_b));

        song.begin_bindings(1);
        song.set_selected(true);
        assert!(!is_selected(&row_a));
    }

    fn count_notifications(property: &str, apply: impl Fn(&SongModel)) -> u32 {
        let model = SongModel::new(make_track("song1"));
        let count = Rc::new(Cell::new(0u32));
        let counter = count.clone();
        model.connect_notify_local(Some(property), move |_, _| {
            counter.set(counter.get() + 1);
        });
        apply(&model);
        count.get()
    }

    #[test]
    fn test_state_setters_do_not_notify_when_unchanged() {
        // Song state is re-seeded in bulk on most app events, usually with the
        // values the model already holds. Listeners on these properties can be
        // expensive, so a no-op set must stay silent.
        assert_eq!(
            count_notifications("liked", |m| {
                m.set_liked(false);
                m.set_liked(false);
            }),
            0
        );
        assert_eq!(count_notifications("playing", |m| m.set_playing(false)), 0);
        assert_eq!(
            count_notifications("selected", |m| m.set_selected(false)),
            0
        );
        assert_eq!(
            count_notifications("explicit-filtered", |m| m.set_explicit_filtered(false)),
            0
        );
    }

    #[test]
    fn test_state_setters_notify_once_per_change() {
        assert_eq!(
            count_notifications("liked", |m| {
                m.set_liked(true);
                m.set_liked(true);
                m.set_liked(false);
            }),
            2
        );
    }

    #[test]
    fn test_state_setters_update_values() {
        let model = SongModel::new(make_track("song1"));
        model.set_liked(true);
        model.set_playing(true);
        model.set_selected(true);
        model.set_explicit_filtered(true);
        assert!(model.get_liked());
        assert!(model.get_playing());
        assert!(model.get_selected());
        assert!(model.property::<bool>("explicit-filtered"));
    }
}
