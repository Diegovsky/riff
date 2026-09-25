use std::cell::Cell;
use std::sync::Arc;

use gdk::Rectangle;
use gettextrs::gettext;
use gio::MenuModel;
use glib::subclass::InitializingObject;
use gtk::graphene::Point;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use libadwaita::prelude::*;
use riff_api::ApiService;

use crate::app::components::utils::{decode_px, set_css_class};
use crate::app::components::{display_add_css_provider, labels, SubtitleLinksBox};
use crate::app::load;
use crate::app::models::{SongModel, Track};

/// Height of every track list row, tracks and disc headers alike.
pub const ROW_HEIGHT_PX: i32 = 56;

pub const COVER_SIZE: i32 = 40;

// Responsive breakpoints: the row pixel width at or below which a column
// hides. When the width decreases album hides first, then the track time:
//
//   width > 600          cover  title/artist | album   (like)  time  menu
//   400 < width <= 600   cover  title/artist           (like)  time  menu
//   width <= 400         cover  title/artist           (like)        menu
//
// To add a breakpoint: add a const here, a RowWidth variant, register it in
// TrackRow::setup (widest first), map it in the current-breakpoint handler,
// and act on it in TrackRow::update_layout.
const HIDE_ALBUM_AT_PX: f64 = 600.0;
const HIDE_TIME_AT_PX: f64 = 400.0;

/// Room for like_btn (34px `button.circular` plus a gap) at the end of the
/// rightmost text column while it shows.
const LIKE_RESERVE_PX: i32 = 34 + 6;

/// Classes shared by the artist and album labels (song_album sets them in
/// the .blp).
const SECONDARY_TEXT_CLASSES: &[&str] = &["subtitle", "dimmed"];

const LINK_CLASS: &str = "song__link";
const LINK_HOVER_CLASS: &str = "song__link--hover";

/// Which breakpoint the row is in (see HIDE_ALBUM_AT_PX).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RowWidth {
    #[default]
    Wide,
    Medium,
    Narrow,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RowOptions {
    pub show_cover: bool,
    pub show_album: bool,
}

fn activate_row_action(widget: &gtk::Widget, action: &str) {
    if let Some(row) = widget.ancestor(TrackRow::static_type()) {
        let _ = row.activate_action(action, None);
    }
}

fn wire_link_label(label: &gtk::Label, action_name: String) {
    let click = gtk::GestureClick::new();
    // Claim the click so the row doesn't also play the track.
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    click.connect_released(move |gesture, _, _, _| {
        let Some(label) = gesture.widget() else {
            return;
        };
        if label.has_css_class(LINK_CLASS) {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            activate_row_action(&label, &action_name);
        }
    });
    label.add_controller(click);

    let motion = gtk::EventControllerMotion::new();
    motion.connect_enter(|motion, _, _| {
        if let Some(label) = motion.widget() {
            label.add_css_class(LINK_HOVER_CLASS);
        }
    });
    motion.connect_leave(|motion| {
        if let Some(label) = motion.widget() {
            label.remove_css_class(LINK_HOVER_CLASS);
        }
    });
    label.add_controller(motion);
}

mod imp {
    use super::*;

    pub const PLAYING_CLASS: &str = "song--playing";
    pub const LIKED_CLASS: &str = "song--liked";
    pub const UNPLAYABLE_CLASS: &str = "song--unplayable";

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/track_row.ui")]
    pub struct TrackRow {
        #[template_child]
        pub card: TemplateChild<libadwaita::BreakpointBin>,
        #[template_child]
        pub song_index: TemplateChild<gtk::Label>,
        #[template_child]
        pub song_cover: TemplateChild<gtk::Image>,
        #[template_child]
        pub song_checkbox: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub title_artist_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub song_title: TemplateChild<gtk::Label>,
        #[template_child]
        pub song_artist: TemplateChild<SubtitleLinksBox>,
        #[template_child]
        pub song_album: TemplateChild<gtk::Label>,
        #[template_child]
        pub like_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub song_length: TemplateChild<gtk::Label>,
        #[template_child]
        pub menu_btn: TemplateChild<gtk::MenuButton>,

        // Inputs to update_layout.
        pub(super) width: Cell<RowWidth>,
        pub(super) hovered: Cell<bool>,
        pub(super) show_album: Cell<bool>,

        // Inputs to update_unplayable.
        pub(super) playable: Cell<bool>,
        pub(super) explicit_filtered: Cell<bool>,

        pub(super) skeleton: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TrackRow {
        const NAME: &'static str = "TrackRow";
        type Type = super::TrackRow;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    lazy_static! {
        static ref PROPERTIES: [glib::ParamSpec; 5] = [
            glib::ParamSpecBoolean::builder("playing").build(),
            glib::ParamSpecBoolean::builder("selected").build(),
            glib::ParamSpecBoolean::builder("liked").build(),
            glib::ParamSpecBoolean::builder("playable")
                .default_value(true)
                .build(),
            glib::ParamSpecBoolean::builder("explicit-filtered").build(),
        ];
    }

    impl ObjectImpl for TrackRow {
        fn properties() -> &'static [glib::ParamSpec] {
            &*PROPERTIES
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();
            let on: bool = value.get().expect("boolean property");
            match pspec.name() {
                "playing" => set_css_class(&*obj, PLAYING_CLASS, on),
                "selected" => self.song_checkbox.set_active(on),
                "liked" => {
                    set_css_class(&*obj, LIKED_CLASS, on);
                    if on {
                        self.like_btn.set_icon_name("starred-symbolic");
                        self.like_btn.set_tooltip_text(Some(&labels::UNLIKE));
                    } else {
                        self.like_btn.set_icon_name("non-starred-symbolic");
                        self.like_btn.set_tooltip_text(Some(&labels::LIKE));
                    }
                    obj.update_layout();
                }
                "playable" => {
                    self.playable.set(on);
                    obj.update_unplayable();
                }
                "explicit-filtered" => {
                    self.explicit_filtered.set(on);
                    obj.update_unplayable();
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();
            match pspec.name() {
                "playing" => obj.has_css_class(PLAYING_CLASS).to_value(),
                "selected" => self.song_checkbox.is_active().to_value(),
                "liked" => obj.has_css_class(LIKED_CLASS).to_value(),
                "playable" => self.playable.get().to_value(),
                "explicit-filtered" => self.explicit_filtered.get().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.playable.set(true);
            self.card.set_height_request(ROW_HEIGHT_PX);
            self.song_cover.set_pixel_size(COVER_SIZE);
            self.obj().setup();
        }
    }

    impl WidgetImpl for TrackRow {}
    impl BoxImpl for TrackRow {}
}

glib::wrapper! {
    pub struct TrackRow(ObjectSubclass<imp::TrackRow>) @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for TrackRow {
    fn default() -> Self {
        Self::new()
    }
}

impl TrackRow {
    pub fn new() -> Self {
        display_add_css_provider(resource!("/components/track_row.css"));
        glib::Object::new()
    }

    fn setup(&self) {
        let imp = self.imp();

        let max_width = |px| {
            libadwaita::Breakpoint::new(libadwaita::BreakpointCondition::new_length(
                libadwaita::BreakpointConditionLengthType::MaxWidth,
                px,
                libadwaita::LengthUnit::Px,
            ))
        };
        let hide_album = max_width(HIDE_ALBUM_AT_PX);
        let hide_time = max_width(HIDE_TIME_AT_PX);
        imp.card.add_breakpoint(hide_album);
        imp.card.add_breakpoint(hide_time.clone());
        imp.card.connect_current_breakpoint_notify(clone!(
            #[weak(rename_to = row)]
            self,
            move |card| {
                let width = match card.current_breakpoint() {
                    None => RowWidth::Wide,
                    Some(current) if current == hide_time => RowWidth::Narrow,
                    Some(_) => RowWidth::Medium,
                };
                row.imp().width.set(width);
                row.update_layout();
            }
        ));

        let hover = gtk::EventControllerMotion::new();
        hover.connect_enter(clone!(
            #[weak(rename_to = row)]
            self,
            move |_, _, _| row.set_hovered(true)
        ));
        hover.connect_leave(clone!(
            #[weak(rename_to = row)]
            self,
            move |_| row.set_hovered(false)
        ));
        self.add_controller(hover);

        let right_click = gtk::GestureClick::builder()
            .button(gdk::BUTTON_SECONDARY)
            .build();
        right_click.connect_pressed(clone!(
            #[weak(rename_to = row)]
            self,
            move |_, _, x, y| row.show_menu(x, y)
        ));
        self.add_controller(right_click);

        imp.like_btn
            .connect_clicked(|button| activate_row_action(button.upcast_ref(), "song.like"));
        wire_link_label(&imp.song_album, "song.view_album".to_string());
    }

    fn set_hovered(&self, hovered: bool) {
        self.imp().hovered.set(hovered);
        self.update_layout();
    }

    /// Shows or hides columns for the current breakpoint
    fn update_layout(&self) {
        let imp = self.imp();
        let width = imp.width.get();

        let album_visible = imp.show_album.get() && width == RowWidth::Wide;
        imp.song_album.set_visible(album_visible);
        imp.song_length.set_visible(width != RowWidth::Narrow);

        let like_visible =
            !imp.skeleton.get() && (imp.hovered.get() || self.has_css_class(imp::LIKED_CLASS));
        imp.like_btn.set_visible(like_visible);
        let reserve = |rightmost: bool| {
            if like_visible && rightmost {
                LIKE_RESERVE_PX
            } else {
                0
            }
        };
        imp.song_album.set_margin_end(reserve(album_visible));
        imp.title_artist_box.set_margin_end(reserve(!album_visible));
    }

    fn update_unplayable(&self) {
        let imp = self.imp();
        let reason = if imp.explicit_filtered.get() {
            Some(gettext("Explicit Content is Disabled"))
        } else if !imp.playable.get() {
            Some(gettext("Not Available in Your Region"))
        } else {
            None
        };
        set_css_class(self, imp::UNPLAYABLE_CLASS, reason.is_some());
        self.set_tooltip_text(reason.as_deref());
    }

    pub fn set_actions(&self, actions: Option<&gio::ActionGroup>) {
        self.insert_action_group("song", actions);
    }

    pub fn set_menu(&self, menu: Option<&MenuModel>) {
        let menu_btn = &self.imp().menu_btn;
        menu_btn.set_menu_model(menu);
        menu_btn.set_visible(menu.is_some());
        if let Some(popover) = menu_btn.popover() {
            popover.set_has_arrow(false);
            popover.connect_closed(|popover| popover.set_pointing_to(None));
        }
    }

    fn show_menu(&self, x: f64, y: f64) {
        let menu_btn = &*self.imp().menu_btn;
        let Some(popover) = menu_btn.popover() else {
            return;
        };
        let Some(origin) = self.compute_point(menu_btn, &Point::zero()) else {
            return;
        };
        popover.set_pointing_to(Some(&Rectangle::new(
            (origin.x() as f64 + x) as i32,
            (origin.y() as f64 + y) as i32,
            1,
            1,
        )));
        popover.popup();
    }

    pub fn set_disc_position(&self, is_disc_start: bool, is_disc_end: bool) {
        set_css_class(self, "song--disc-start", is_disc_start);
        set_css_class(self, "song--disc-end", is_disc_end);
    }

    pub fn bind(&self, model: &SongModel, api_service: Arc<ApiService>, options: RowOptions) {
        let imp = self.imp();
        let song = model.description().clone();
        self.set_skeleton(false);

        model.bind_title(&*imp.song_title, "label");
        model.bind_duration(&*imp.song_length, "label");
        model.bind_playing(self, "playing");
        model.bind_selected(self, "selected");
        model.bind_liked(self, "liked");
        model.bind_playable(self, "playable");
        model.bind_explicit_filtered(self, "explicit-filtered");

        self.set_artist_links(&song);

        if options.show_cover {
            self.load_cover(&song, api_service);
        } else {
            model.bind_index(&*imp.song_index, "label");
        }

        imp.show_album.set(options.show_album);
        if options.show_album {
            self.set_album_link(&song);
        }
        self.update_layout();
    }

    pub fn bind_skeleton(&self) {
        let imp = self.imp();
        self.set_skeleton(true);
        for class in [imp::PLAYING_CLASS, imp::LIKED_CLASS, imp::UNPLAYABLE_CLASS] {
            self.remove_css_class(class);
        }
        self.set_tooltip_text(None);
        imp.song_checkbox.set_active(false);
        imp.song_cover.set_paintable(None::<&gdk::Paintable>);
        // Placeholder text sizes the blocks; it's never visible.
        imp.song_title.set_label("Loading track title");
        imp.song_artist.clear_links();
        let artist = gtk::Label::new(Some("Artist name"));
        artist.set_css_classes(SECONDARY_TEXT_CLASSES);
        imp.song_artist.append_link(&artist);
        imp.song_length.set_label("0∶00");
        imp.show_album.set(false);
        self.set_actions(None);
        // No menu, but the button keeps its space (hidden by the CSS), so
        // the time block sits where a real row's time does.
        self.set_menu(None);
        imp.menu_btn.set_visible(true);
        self.update_layout();
    }

    fn set_skeleton(&self, skeleton: bool) {
        let imp = self.imp();
        imp.skeleton.set(skeleton);
        set_css_class(self, "song--skeleton", skeleton);
        // A block the size of the text, not of the whole column.
        imp.song_title.set_halign(if skeleton {
            gtk::Align::Start
        } else {
            gtk::Align::Fill
        });
    }

    fn load_cover(&self, song: &Track, api_service: Arc<ApiService>) {
        let Some(url) = song.art.best_for_width((COVER_SIZE * 2) as u32) else {
            return;
        };
        let url = url.to_owned();
        let row = self.downgrade();
        let tag = load::visible();
        let size = decode_px(COVER_SIZE);
        glib::MainContext::default().spawn_local_with_priority(
            glib::Priority::DEFAULT_IDLE,
            async move {
                let Some(row) = row.upgrade() else { return };
                if row.imp().skeleton.get() {
                    return; // recycled as a placeholder meanwhile
                }
                if let Some(texture) = api_service.load_image(&url, size, size, tag).await {
                    row.imp().song_cover.set_paintable(Some(&texture));
                }
            },
        );
    }

    fn set_artist_links(&self, song: &Track) {
        let links_box = &self.imp().song_artist;
        links_box.clear_links();

        for (i, artist) in song.artists.iter().enumerate() {
            if i > 0 {
                let separator = gtk::Label::new(Some(", "));
                separator.set_css_classes(SECONDARY_TEXT_CLASSES);
                links_box.append_link(&separator);
            }

            let label = gtk::Label::new(Some(&artist.name));
            label.set_css_classes(SECONDARY_TEXT_CLASSES);
            if !artist.rri.id.is_empty() {
                label.add_css_class(LINK_CLASS);
                wire_link_label(&label, format!("song.view_artist_{}", artist.rri.id));
            }
            links_box.append_link(&label);
        }
    }

    fn set_album_link(&self, song: &Track) {
        let label = &self.imp().song_album;
        let album = song.album.as_ref();
        label.set_label(album.map_or("", |a| a.name.as_str()));
        set_css_class(
            &**label,
            LINK_CLASS,
            album.is_some_and(|a| !a.rri.id.is_empty()),
        );
    }
}
