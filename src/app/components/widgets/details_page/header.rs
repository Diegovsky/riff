use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use super::{SubtitleLinksBox, HEADER_IMAGE_SIZE};
use crate::app::components::utils::add_hover_class;
use crate::app::components::{labels, ExpandBehavior, SegmentedButton};

/// Between a subtitle and its detail (see `PageModel::get_subtitle_detail`).
pub(super) const DETAIL_SEPARATOR: &str = " · ";

const SUBTITLE_LINK_CLASS: &str = "subtitle-link";
const SUBTITLE_LINK_HOVER_CLASS: &str = "subtitle-link--hover";

/// Controls the shape of the artwork in the details header.
/// - `Square`: used for albums/playlists (rendered with rounded card corners).
/// - `Circle`: used for artist avatars (fully circular clip).
#[derive(Clone, Copy, PartialEq)]
pub enum HeaderImageShape {
    Square,
    Circle,
}

// GObject widget (template-backed)

mod imp {
    use super::*;

    /// Inner GObject struct for the composite template in `header.blp`.
    ///
    /// One widget tree serves both layouts; a breakpoint flips `header_box`
    /// between horizontal (artwork beside text) and vertical (above it).
    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/details_header.ui")]
    pub struct DetailsHeaderWidget {
        #[template_child]
        pub breakpoint_bin: TemplateChild<libadwaita::BreakpointBin>,

        #[template_child]
        pub image_box: TemplateChild<gtk::Box>,

        #[template_child]
        pub image: TemplateChild<gtk::Image>,

        #[template_child]
        pub caption_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub title_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub subtitle_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub subtitle_links_box: TemplateChild<SubtitleLinksBox>,

        #[template_child]
        pub play_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub shuffle_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub menu_button: TemplateChild<gtk::MenuButton>,

        #[template_child]
        pub like_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub share_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub button_box: TemplateChild<gtk::Box>,

        #[template_child]
        pub edit_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DetailsHeaderWidget {
        const NAME: &'static str = "DetailsHeaderWidget";
        type Type = super::DetailsHeaderWidget;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DetailsHeaderWidget {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_overflow(gtk::Overflow::Hidden);
        }

        fn dispose(&self) {
            // Unparent template children before finalization to avoid a GTK
            // warning (they parent directly to this widget).
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for DetailsHeaderWidget {
        /// Report natural height as the minimum too. The child `AdwBreakpointBin`
        /// is pinned to a 1px minimum so the breakpoint can shrink; without this
        /// the scrolled box could squeeze the header down to that 1px.
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let bin = self.breakpoint_bin.get();
            let (min, nat, min_baseline, nat_baseline) = bin.measure(orientation, for_size);

            if orientation == gtk::Orientation::Vertical {
                return (nat, nat, min_baseline, nat_baseline);
            }

            (min, nat, min_baseline, nat_baseline)
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            // Fill this widget with its child; the BreakpointBin evaluates the
            // breakpoint for `width` and lays out its content.
            self.breakpoint_bin.allocate(width, height, baseline, None);
        }
    }
}

glib::wrapper! {
    pub struct DetailsHeaderWidget(ObjectSubclass<imp::DetailsHeaderWidget>) @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

/// High-level wrapper around `DetailsHeaderWidget`.
///
/// Provides a clean API for detail pages to set artwork, titles, and action
/// buttons without touching GObject internals directly.
pub struct DetailsHeader {
    widget: DetailsHeaderWidget,
    like_pin: LikePinState,
}

#[derive(Default)]
struct LikePinState {
    seg: RefCell<Option<(SegmentedButton, gtk::Button, gtk::Button)>>,
    like_visible: Cell<bool>,
    pin_enabled: Cell<bool>,
    liked: Cell<bool>,
}

impl DetailsHeader {
    pub fn for_widget(widget: DetailsHeaderWidget, shape: HeaderImageShape) -> Self {
        let imp = widget.imp();
        if shape == HeaderImageShape::Circle {
            imp.image.add_css_class("details-header__image--circular");
            imp.image_box
                .add_css_class("details-header__image--circular");
        }

        let like_pin = LikePinState {
            like_visible: Cell::new(true),
            ..Default::default()
        };
        Self { widget, like_pin }
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.widget.upcast_ref()
    }

    // Text content
    //
    // Caption/subtitle use `visible` (not opacity) so empty values take no space.

    pub fn set_title(&self, title: &str) {
        self.widget.imp().title_label.set_label(title);
    }

    pub fn set_caption(&self, caption: &str) {
        let imp = self.widget.imp();
        imp.caption_label.set_label(caption);
        imp.caption_label.set_visible(!caption.is_empty());
    }

    pub fn set_caption_visible(&self, visible: bool) {
        self.widget.imp().caption_label.set_visible(visible);
    }

    pub fn set_subtitle(&self, subtitle: &str) {
        let imp = self.widget.imp();
        imp.subtitle_label.set_label(subtitle);
        imp.subtitle_label.set_visible(!subtitle.is_empty());
        // A plain subtitle and the links box are mutually exclusive.
        imp.subtitle_links_box.set_visible(false);
    }

    pub fn get_title_text(&self) -> String {
        self.widget.imp().title_label.label().to_string()
    }

    // Artwork

    /// Display a themed icon as fallback artwork (e.g. when no image is available).
    pub fn set_default_icon(&self, icon_name: &str) {
        let display = gdk::Display::default().unwrap();
        let scale = self.widget.scale_factor();
        let icon = gtk::IconTheme::for_display(&display).lookup_icon(
            icon_name,
            &[],
            HEADER_IMAGE_SIZE,
            scale,
            gtk::TextDirection::None,
            gtk::IconLookupFlags::empty(),
        );
        let imp = self.widget.imp();
        imp.image.set_paintable(Some(&icon));
        imp.image_box
            .add_css_class("details-header__image-box--icon");
    }

    // Action button state

    /// Update the play button icon/tooltip to reflect current playback state.
    pub fn set_playing(&self, is_playing: bool) {
        let icon = if is_playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        };
        let tooltip = if is_playing {
            gettext("Pause")
        } else {
            gettext("Play")
        };
        let play_button = &self.widget.imp().play_button;
        play_button.set_icon_name(icon);
        play_button.set_tooltip_text(Some(&tooltip));
    }

    /// Update the like button icon to reflect saved/unsaved state.
    pub fn set_liked(&self, is_liked: bool) {
        let icon = if is_liked {
            "starred-symbolic"
        } else {
            "non-starred-symbolic"
        };
        self.widget.imp().like_button.set_icon_name(icon);
        if let Some((_, like, _)) = &*self.like_pin.seg.borrow() {
            like.set_icon_name(icon);
        }
        self.like_pin.liked.set(is_liked);
        self.sync_like_controls();
    }

    /// Show or hide the like button.
    pub fn set_like_visible(&self, visible: bool) {
        self.like_pin.like_visible.set(visible);
        if self.like_pin.seg.borrow().is_none() {
            self.widget.imp().like_button.set_visible(visible);
        }
        self.sync_like_controls();
    }

    /// Override the like button's tooltip.
    pub fn set_like_tooltip(&self, tooltip: &str) {
        self.widget
            .imp()
            .like_button
            .set_tooltip_text(Some(tooltip));
    }

    // Signal connections

    /// Connect a handler to the play button. Also makes the button visible.
    pub fn connect_play<F: Fn() + 'static>(&self, f: F) {
        let button = &self.widget.imp().play_button;
        button.set_visible(true);
        button.connect_clicked(move |_| f());
    }

    /// Connect a handler to the shuffle button. Also makes the button visible.
    pub fn connect_shuffle<F: Fn() + 'static>(&self, f: F) {
        let button = &self.widget.imp().shuffle_button;
        button.set_visible(true);
        button.connect_clicked(move |_| f());
    }

    /// Sections of (id, label) entries; `on_activate` gets the id. Hidden when
    /// empty.
    pub fn set_menu<F: Fn(&str) + 'static>(
        &self,
        sections: &[Vec<(String, String)>],
        on_activate: F,
    ) {
        let button = &self.widget.imp().menu_button;
        let on_activate = Rc::new(on_activate);
        let actions = gio::SimpleActionGroup::new();
        let menu = gio::Menu::new();
        for entries in sections.iter().filter(|entries| !entries.is_empty()) {
            let section = gio::Menu::new();
            for (id, label) in entries {
                let action = gio::SimpleAction::new(id, None);
                let on_activate = on_activate.clone();
                let action_id = id.clone();
                action.connect_activate(move |_, _| on_activate(&action_id));
                actions.add_action(&action);
                section.append(Some(label), Some(&format!("header.{id}")));
            }
            menu.append_section(None, &section);
        }
        button.insert_action_group("header", Some(&actions));
        button.set_menu_model(Some(&menu));
        button.set_visible(menu.n_items() > 0);
    }

    /// Connect a handler to the like/save button. Also makes the button visible.
    pub fn connect_liked<F: Fn() + 'static>(&self, f: F) {
        let button = &self.widget.imp().like_button;
        button.set_visible(true);
        button.connect_clicked(move |_| f());
    }

    /// Connect a handler to the share button. Also makes the button visible.
    pub fn connect_share<F: Fn() + 'static>(&self, f: F) {
        let button = &self.widget.imp().share_button;
        button.set_visible(true);
        button.connect_clicked(move |_| f());
    }

    /// Connect a handler to the edit button. Also makes the button visible.
    #[allow(dead_code)]
    pub fn connect_edit<F: Fn() + 'static>(&self, f: F) {
        let button = &self.widget.imp().edit_button;
        button.set_visible(true);
        button.connect_clicked(move |_| f());
    }

    /// Update the pin button icon and tooltip to reflect pinned state.
    pub fn set_pinned(&self, is_pinned: bool) {
        let (icon, tooltip) = if is_pinned {
            ("view-pin-symbolic", &*labels::UNPIN_FROM_SIDEBAR)
        } else {
            ("view-pin-outline-symbolic", &*labels::PIN_TO_SIDEBAR)
        };
        if let Some((_, _, pin)) = &*self.like_pin.seg.borrow() {
            pin.set_icon_name(icon);
            pin.set_tooltip_text(Some(tooltip));
        }
    }

    pub fn set_pin_visible(&self, visible: bool) {
        if let Some((seg, _, _)) = &*self.like_pin.seg.borrow() {
            seg.set_icon_visible(1, visible);
        }
        self.sync_like_controls();
    }

    /// Hide the standalone like button.
    pub fn set_pin_enabled(&self, enabled: bool) {
        self.like_pin.pin_enabled.set(enabled);
        self.sync_like_controls();
    }

    fn sync_like_controls(&self) {
        let seg = self.like_pin.seg.borrow();
        let Some((seg, _, pin)) = &*seg else {
            return;
        };
        let like_visible = self.like_pin.like_visible.get();
        let like_button = &self.widget.imp().like_button;
        let pin_enabled = self.like_pin.pin_enabled.get();
        like_button.set_visible(like_visible && !pin_enabled);
        seg.widget()
            .set_visible(pin_enabled && (like_visible || pin.is_visible()));
        seg.set_icon_visible(0, like_visible);
        // Keep the pin segment revealed for liked items, and when there is
        // no like segment to hover.
        let behavior = if like_visible && !self.like_pin.liked.get() {
            ExpandBehavior::OnHover
        } else {
            ExpandBehavior::AlwaysExpanded
        };
        if seg.expand_behavior() != behavior {
            seg.set_expand_behavior(behavior);
        }
    }

    /// Add a like+pin segmented control after the standalone pin button.
    ///
    /// The like segment is always visible and acts as the expand trigger;
    /// hovering reveals the pin segment. Returns `(like_segment, pin_segment)`
    /// for wiring actions and state updates.
    pub fn add_like_pin_segmented_button(&self) -> (gtk::Button, gtk::Button) {
        let seg = SegmentedButton::new(ExpandBehavior::OnHover);
        seg.widget().set_valign(gtk::Align::Center);

        let like = seg.add_icon("non-starred-symbolic", &gettext("Add to Library"), || {});
        let pin = seg.add_icon("view-pin-outline-symbolic", &labels::PIN_TO_SIDEBAR, || {});

        let imp = self.widget.imp();
        imp.button_box
            .insert_child_after(seg.widget(), Some(&*imp.like_button));
        *self.like_pin.seg.borrow_mut() = Some((seg, like.clone(), pin.clone()));
        self.sync_like_controls();
        (like, pin)
    }


    pub fn set_subtitle_links<F: Fn(&str) + 'static>(
        &self,
        artists: &[(String, String)],
        detail: Option<&str>,
        on_clicked: F,
    ) {
        let imp = self.widget.imp();
        let links_box = &*imp.subtitle_links_box;

        // Clear any previous children.
        links_box.clear_links();

        if artists.is_empty() {
            links_box.set_visible(false);
            return;
        }

        // Show the links box in place of the plain subtitle label.
        links_box.set_visible(true);
        imp.subtitle_label.set_visible(false);

        let on_clicked = Rc::new(on_clicked);
        for (i, (id, name)) in artists.iter().enumerate() {
            if i > 0 {
                let separator = gtk::Label::new(Some(", "));
                separator.add_css_class("body");
                links_box.append_link(&separator);
            }


            let label = gtk::Label::builder()
                .label(name)
                .focusable(true)
                .accessible_role(gtk::AccessibleRole::Link)
                .css_classes(["body", SUBTITLE_LINK_CLASS])
                .build();

            let click = gtk::GestureClick::new();
            let click_id = id.clone();
            let cb = Rc::clone(&on_clicked);
            click.connect_released(move |gesture, _, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                cb(&click_id);
            });
            label.add_controller(click);

            let keys = gtk::EventControllerKey::new();
            let id = id.clone();
            let cb = Rc::clone(&on_clicked);
            keys.connect_key_pressed(move |_, key, _, _| match key {
                gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter | gtk::gdk::Key::space => {
                    cb(&id);
                    gtk::glib::Propagation::Stop
                }
                _ => gtk::glib::Propagation::Proceed,
            });
            label.add_controller(keys);

            add_hover_class(&label, SUBTITLE_LINK_HOVER_CLASS);

            links_box.append_link(&label);
        }

        if let Some(detail) = detail {
            let detail_label = gtk::Label::new(Some(&format!("{DETAIL_SEPARATOR}{detail}")));
            detail_label.add_css_class("body");
            links_box.append_link(&detail_label);
        }
    }

    // Weak references

    /// Weak reference to the underlying widget, for moving into async tasks.
    pub fn widget_weak(&self) -> gtk::glib::WeakRef<DetailsHeaderWidget> {
        self.widget.downgrade()
    }
}

/// Ensure the GObject type is registered (called at app startup).
pub fn expose_widgets() {
    DetailsHeaderWidget::static_type();
}
