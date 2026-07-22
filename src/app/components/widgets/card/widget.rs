//! Card widget — a reusable artwork + label tile used throughout the app.
//!
//! Each card displays an image (album cover, artist photo, playlist art) with
//! optional title/subtitle labels. Cards support three layouts (vertical, image-only,
//! horizontal), three sizes (small, medium, large), and two image shapes (square, round).
//!
//! Cards are typically arranged in a `FlowBox` via `CardList` and bound to a
//! `CardModel` from the app state.

use crate::app::components::display_add_css_provider;
use crate::app::dispatch::Worker;
use crate::app::loader::ImageLoader;
use crate::app::models::{CardLayout, CardModel, CardSize};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

// Constants

/// Resolution (in pixels) at which card artwork is fetched from the server.
/// Also used by model conversions to select the best source image URL.
pub const IMAGE_SIZE: u32 = 180;

/// Vertical gap (in pixels) between the image and the label box in vertical layout.
const LABEL_GAP: i32 = 6;

/// Horizontal gap (in pixels) between the image and the label box in horizontal layout.
const HORIZONTAL_GAP: i32 = 12;

/// Width multiplier for the label area in horizontal layout (relative to image size).
const HORIZONTAL_LABEL_WIDTH_SCALE: f32 = 1.8;

/// Cards at or below this position in the list load immediately; the rest yield
/// to the main loop via `idle_add_local_once` to avoid startup jank.
const VISIBLE_THRESHOLD: u32 = 15;

// Enums

/// Controls whether the card artwork is circular or square.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageShape {
    Round,
    Square,
}

// GObject implementation

mod imp {
    use super::*;
    use std::cell::Cell;
    use std::cell::RefCell;

    #[derive(Debug, CompositeTemplate)]
    #[template(file = "src/app/components/widgets/card/card.blp")]
    pub struct CardWidget {
        #[template_child]
        pub label_box: TemplateChild<gtk::Box>,

        #[template_child]
        pub title_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub subtitle_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub cover_image: TemplateChild<gtk::Picture>,

        /// Current pixel size of the card image.
        pub icon_size: Cell<i32>,
        /// Current layout mode.
        pub layout: Cell<CardLayout>,
        /// Spotify ID for the item this card represents (needed for click handling).
        pub card_id: RefCell<String>,
    }

    impl Default for CardWidget {
        fn default() -> Self {
            Self {
                label_box: Default::default(),
                title_label: Default::default(),
                subtitle_label: Default::default(),
                cover_image: Default::default(),
                icon_size: Cell::new(CardSize::Large.pixel_size()),
                layout: Cell::new(CardLayout::Vertical),
                card_id: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CardWidget {
        const NAME: &'static str = "CardWidget";
        type Type = super::CardWidget;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.set_css_name("cardwidget");
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for CardWidget {
        // Required for composite templates with ParentType = gtk::Widget.
        // GTK does not automatically unparent children of plain widgets.
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for CardWidget {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let px = self.icon_size.get();
            let layout = self.layout.get();

            if orientation == gtk::Orientation::Horizontal {
                let w = match layout {
                    CardLayout::Horizontal => {
                        px + HORIZONTAL_GAP + (HORIZONTAL_LABEL_WIDTH_SCALE * px as f32) as i32
                    }
                    _ => px,
                };
                return (w, w, -1, -1);
            }

            // Vertical measurement.
            match layout {
                CardLayout::Horizontal => {
                    let (label_min, _, _, _) = self.label_box.measure(
                        gtk::Orientation::Vertical,
                        (HORIZONTAL_LABEL_WIDTH_SCALE * px as f32) as i32,
                    );
                    let h = px.max(label_min);
                    (h, h, -1, -1)
                }
                CardLayout::ImageOnly => (px, px, -1, -1),
                CardLayout::Vertical => {
                    let (label_min, label_nat, _, _) =
                        self.label_box.measure(gtk::Orientation::Vertical, px);
                    let h = px + LABEL_GAP + label_nat;
                    (px + LABEL_GAP + label_min, h, -1, -1)
                }
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let px = self.icon_size.get();
            let layout = self.layout.get();

            match layout {
                CardLayout::Vertical => {
                    let img_x = (width - px) / 2;
                    let transform = gtk::gsk::Transform::new()
                        .translate(&gtk::graphene::Point::new(img_x as f32, 0.0));
                    self.cover_image.allocate(px, px, -1, Some(transform));

                    let label_h = height - px - LABEL_GAP;
                    if label_h > 0 {
                        let transform = gtk::gsk::Transform::new().translate(
                            &gtk::graphene::Point::new(img_x as f32, (px + LABEL_GAP) as f32),
                        );
                        self.label_box.allocate(px, label_h, -1, Some(transform));
                    }
                }
                CardLayout::ImageOnly => {
                    let img_x = (width - px) / 2;
                    let transform = gtk::gsk::Transform::new()
                        .translate(&gtk::graphene::Point::new(img_x as f32, 0.0));
                    self.cover_image.allocate(px, px, -1, Some(transform));
                }
                CardLayout::Horizontal => {
                    let img_y = (height - px) / 2;
                    let img_transform = gtk::gsk::Transform::new()
                        .translate(&gtk::graphene::Point::new(0.0, img_y as f32));
                    self.cover_image.allocate(px, px, -1, Some(img_transform));

                    let label_w = width - px - HORIZONTAL_GAP;
                    if label_w > 0 {
                        let (label_min, _, _, _) =
                            self.label_box.measure(gtk::Orientation::Vertical, label_w);
                        let label_h = label_min.min(height);
                        let label_y = (height - label_h) / 2;
                        let transform =
                            gtk::gsk::Transform::new().translate(&gtk::graphene::Point::new(
                                (px + HORIZONTAL_GAP) as f32,
                                label_y as f32,
                            ));
                        self.label_box
                            .allocate(label_w, label_h, -1, Some(transform));
                    }
                }
            }
        }
    }
}

// Public API

glib::wrapper! {
    /// A card widget displaying artwork with optional title/subtitle labels.
    ///
    /// Cards are the primary visual unit in grid and list views. They render
    /// a cover image at a configurable size and layout, with a skeleton loading
    /// animation until content arrives.
    pub struct CardWidget(ObjectSubclass<imp::CardWidget>) @extends gtk::Widget;
}

impl CardWidget {
    /// Create a new empty card with the given image shape and layout.
    pub fn new(shape: ImageShape, layout: CardLayout) -> Self {
        display_add_css_provider(resource!("/components/card.css"));
        let widget: Self = glib::Object::new();
        widget.add_css_class("container");
        match shape {
            ImageShape::Round => widget.add_css_class("card--round"),
            ImageShape::Square => widget.add_css_class("card--square"),
        }
        widget.add_css_class(layout.css_class());
        widget
    }

    /// Create a card pre-bound to a model, ready for display.
    pub fn for_model(
        model: &CardModel,
        worker: Worker,
        shape: ImageShape,
        layout: CardLayout,
        size: CardSize,
    ) -> Self {
        let widget = Self::new(shape, layout);
        widget.set_image_size(size);
        widget.set_layout(layout);
        widget.bind(model, worker);
        widget
    }

    /// Update the image size, replacing the CSS class and triggering a resize.
    pub fn set_image_size(&self, size: CardSize) {
        for s in &[CardSize::Small, CardSize::Medium, CardSize::Large] {
            self.remove_css_class(s.css_class());
        }
        self.add_css_class(size.css_class());
        self.imp().icon_size.set(size.pixel_size());
        self.queue_resize();
    }

    /// Update the layout orientation, adjusting label visibility and alignment.
    pub fn set_layout(&self, layout: CardLayout) {
        for l in &[
            CardLayout::Vertical,
            CardLayout::ImageOnly,
            CardLayout::Horizontal,
        ] {
            self.remove_css_class(l.css_class());
        }
        self.add_css_class(layout.css_class());
        let imp = self.imp();
        imp.layout.set(layout);
        match layout {
            CardLayout::ImageOnly => imp.label_box.set_visible(false),
            CardLayout::Horizontal => {
                imp.label_box.set_visible(true);
                imp.title_label.set_halign(gtk::Align::Start);
                imp.subtitle_label.set_halign(gtk::Align::Start);
            }
            _ => {
                imp.label_box.set_visible(true);
                imp.title_label.set_halign(gtk::Align::Fill);
                imp.subtitle_label.set_halign(gtk::Align::Fill);
            }
        }
        self.queue_resize();
    }

    /// Mark the card as loaded (removes skeleton animation).
    fn set_loaded(&self) {
        self.add_css_class("container--loaded");
    }

    /// The Spotify ID of the item this card represents.
    pub fn card_id(&self) -> String {
        self.imp().card_id.borrow().clone()
    }

    /// Bind this card to a model, loading artwork asynchronously.
    fn bind(&self, model: &CardModel, worker: Worker) {
        let imp = self.imp();
        *imp.card_id.borrow_mut() = model.id();

        // Placeholder cards (empty id) stay in skeleton state.
        if model.id().is_empty() {
            return;
        }

        if let Some(url) = model.image() {
            let weak = self.downgrade();
            let title = model.title();
            let subtitle = model.subtitle();
            let position = model.insertion_position();

            let load = async move {
                if let Some(this) = weak.upgrade() {
                    let loader = ImageLoader::new();
                    let pixbuf = loader
                        .load_remote(&url, "jpg", IMAGE_SIZE as i32, IMAGE_SIZE as i32)
                        .await;
                    if let Some(ref pixbuf) = pixbuf {
                        let texture = gdk::Texture::for_pixbuf(pixbuf);
                        this.imp().cover_image.set_paintable(Some(&texture));
                    }
                    this.imp().title_label.set_label(&title);
                    this.imp().subtitle_label.set_label(&subtitle);
                    this.imp().subtitle_label.set_visible(!subtitle.is_empty());
                    this.set_tooltip_text(Some(&title));
                    this.set_loaded();
                }
            };

            // Visible cards load immediately; off-screen cards yield briefly
            // to avoid blocking the main loop with a burst of disk I/O + decode.
            if position <= VISIBLE_THRESHOLD {
                worker.send_local_task(load);
            } else {
                // Use idle callback instead of linear timeouts — GTK schedules
                // these between frames, loading as fast as possible without jank.
                glib::idle_add_local_once(move || {
                    worker.send_local_task(load);
                });
            }
        } else {
            model
                .bind_property("title", &*imp.title_label, "label")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build();
            model
                .bind_property("subtitle", &*imp.subtitle_label, "label")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build();
            model
                .bind_property("subtitle", &*imp.subtitle_label, "visible")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .transform_to(|_, subtitle: String| Some(!subtitle.is_empty()))
                .build();
            self.set_loaded();
        }
    }
}
