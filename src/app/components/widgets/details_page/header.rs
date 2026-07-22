use std::rc::Rc;

use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use super::HEADER_IMAGE_SIZE;

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

    /// Inner GObject struct for the composite template defined in `header.blp`.
    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/details_header.ui")]
    pub struct DetailsHeaderWidget {
        #[template_child]
        pub image_box: TemplateChild<gtk::Box>,

        #[template_child]
        pub image: TemplateChild<gtk::Picture>,

        #[template_child]
        pub caption_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub title_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub subtitle_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub subtitle_links_box: TemplateChild<gtk::Box>,

        #[template_child]
        pub play_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub shuffle_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub like_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub share_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub info_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub edit_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DetailsHeaderWidget {
        const NAME: &'static str = "DetailsHeaderWidget";
        type Type = super::DetailsHeaderWidget;
        type ParentType = gtk::Box;

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
    }
    impl WidgetImpl for DetailsHeaderWidget {}
    impl BoxImpl for DetailsHeaderWidget {}
}

glib::wrapper! {
    pub struct DetailsHeaderWidget(ObjectSubclass<imp::DetailsHeaderWidget>) @extends gtk::Widget, gtk::Box;
}

/// High-level wrapper around `DetailsHeaderWidget`.
///
/// Provides a clean API for detail pages to set artwork, titles, and action
/// buttons without touching GObject internals directly.
pub struct DetailsHeader {
    widget: DetailsHeaderWidget,
}

impl DetailsHeader {
    pub fn new(shape: HeaderImageShape) -> Self {
        let widget: DetailsHeaderWidget = glib::Object::new();

        widget.imp().image.set_halign(gtk::Align::Center);
        widget.imp().image.set_valign(gtk::Align::Center);
        widget
            .imp()
            .image_box
            .add_css_class("details-header__image-placeholder");

        // Apply shape-specific styling.
        widget.imp().image_box.add_css_class("card");
        match shape {
            HeaderImageShape::Square => {}
            HeaderImageShape::Circle => {
                widget
                    .imp()
                    .image
                    .add_css_class("details-header__image--circular");
                widget
                    .imp()
                    .image_box
                    .add_css_class("details-header__image--circular");
            }
        }

        Self { widget }
    }

    pub fn widget(&self) -> &gtk::Box {
        self.widget.upcast_ref()
    }

    // Text content

    pub fn set_title(&self, title: &str) {
        self.widget.imp().title_label.set_label(title);
    }

    pub fn set_caption(&self, caption: &str) {
        let imp = self.widget.imp();
        imp.caption_label.set_label(caption);
        imp.caption_label
            .set_opacity(if caption.is_empty() { 0.0 } else { 1.0 });
    }

    pub fn set_caption_visible(&self, visible: bool) {
        self.widget
            .imp()
            .caption_label
            .set_opacity(if visible { 1.0 } else { 0.0 });
    }

    pub fn set_subtitle(&self, subtitle: &str) {
        let imp = self.widget.imp();
        imp.subtitle_label.set_label(subtitle);
        imp.subtitle_label
            .set_opacity(if subtitle.is_empty() { 0.0 } else { 1.0 });
        // When setting a plain subtitle, hide the links box
        imp.subtitle_links_box.set_visible(false);
        imp.subtitle_label.set_visible(true);
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
        self.widget.imp().image.set_paintable(Some(&icon));
        self.widget
            .imp()
            .image
            .set_content_fit(gtk::ContentFit::Fill);
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
        self.widget.imp().play_button.set_icon_name(icon);
        self.widget
            .imp()
            .play_button
            .set_tooltip_text(Some(&tooltip));
    }

    /// Update the like button icon to reflect saved/unsaved state.
    pub fn set_liked(&self, is_liked: bool) {
        self.widget.imp().like_button.set_icon_name(if is_liked {
            "starred-symbolic"
        } else {
            "non-starred-symbolic"
        });
    }

    /// Show or hide the like button.
    pub fn set_like_visible(&self, visible: bool) {
        self.widget.imp().like_button.set_visible(visible);
    }

    // Signal connections

    /// Connect a handler to the play button. Also makes the button visible.
    pub fn connect_play<F: Fn() + 'static>(&self, f: F) {
        self.widget.imp().play_button.set_visible(true);
        self.widget.imp().play_button.connect_clicked(move |_| f());
    }

    /// Connect a handler to the shuffle button. Also makes the button visible.
    pub fn connect_shuffle<F: Fn() + 'static>(&self, f: F) {
        self.widget.imp().shuffle_button.set_visible(true);
        self.widget
            .imp()
            .shuffle_button
            .connect_clicked(move |_| f());
    }

    /// Connect a handler to the like/save button. Also makes the button visible.
    pub fn connect_liked<F: Fn() + 'static>(&self, f: F) {
        self.widget.imp().like_button.set_visible(true);
        self.widget.imp().like_button.connect_clicked(move |_| f());
    }

    /// Connect a handler to the info button. Also makes the button visible.
    pub fn connect_info<F: Fn() + 'static>(&self, f: F) {
        self.widget.imp().info_button.set_visible(true);
        self.widget.imp().info_button.connect_clicked(move |_| f());
    }

    /// Connect a handler to the share button. Also makes the button visible.
    pub fn connect_share<F: Fn() + 'static>(&self, f: F) {
        self.widget.imp().share_button.set_visible(true);
        self.widget.imp().share_button.connect_clicked(move |_| f());
    }

    /// Connect a handler to the edit button. Also makes the button visible.
    #[allow(dead_code)]
    pub fn connect_edit<F: Fn() + 'static>(&self, f: F) {
        self.widget.imp().edit_button.set_visible(true);
        self.widget.imp().edit_button.connect_clicked(move |_| f());
    }

    /// Set multiple artist link buttons in the subtitle area.
    /// Each artist is rendered as a clickable button. Buttons are separated by
    /// comma labels: "Artist 1, Artist 2, Artist 3".
    /// The callback receives the artist ID when a button is clicked.
    pub fn set_subtitle_links<F: Fn(&str) + 'static>(
        &self,
        artists: &[(String, String)],
        on_clicked: F,
    ) {
        let imp = self.widget.imp();
        let links_box = &*imp.subtitle_links_box;

        // Clear any previous children
        while let Some(child) = links_box.first_child() {
            links_box.remove(&child);
        }

        if artists.is_empty() {
            links_box.set_visible(false);
            imp.subtitle_label.set_visible(true);
            return;
        }

        // Hide the plain label, show the links box
        imp.subtitle_label.set_visible(false);
        imp.subtitle_label.set_opacity(0.0);
        links_box.set_visible(true);

        let on_clicked = Rc::new(on_clicked);

        for (i, (id, name)) in artists.iter().enumerate() {
            if i > 0 {
                let separator = gtk::Label::new(Some(", "));
                separator.add_css_class("body");
                links_box.append(&separator);
            }

            let button = gtk::Button::builder()
                .label(name)
                .css_classes(["flat", "subtitle-link-button"])
                .build();

            let id = id.clone();
            let cb = Rc::clone(&on_clicked);
            button.connect_clicked(move |_| {
                cb(&id);
            });

            links_box.append(&button);
        }
    }

    // Weak references

    /// Get a weak reference to the underlying widget, suitable for moving into async tasks.
    pub fn widget_weak(&self) -> gtk::glib::WeakRef<DetailsHeaderWidget> {
        self.widget.downgrade()
    }
}

/// Ensure the GObject type is registered (called at app startup).
pub fn expose_widgets() {
    DetailsHeaderWidget::static_type();
}
