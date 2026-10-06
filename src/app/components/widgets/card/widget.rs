//! Card widget - a reusable artwork + label tile used throughout the app.
//!
//! Each card displays an image (album cover, artist photo, playlist art) with
//! optional title/subtitle labels. Cards support three layouts (vertical, image-only,
//! horizontal), three sizes (small, medium, large), and two image shapes (square, round).
//!
//! Cards are typically arranged in a `FlowBox` via `CardList` and bound to a
//! `CardModel` from the app state.

use crate::app::components::display_add_css_provider;
use crate::app::components::utils::{decode_px, set_css_class};
use crate::app::models::{CardLayout, CardModel, CardSize};
use riff_api::ApiService;

use crate::app::load;
use std::sync::Arc;

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

/// Narrowest the label area may shrink to in horizontal layout (the labels
/// ellipsize).
const HORIZONTAL_LABEL_MIN_WIDTH: i32 = 48;

/// In horizontal layout, once the labels are at their minimum the image
/// shrinks too, down to the small card size, so the card fits narrow pages.
fn horizontal_image_px(px: i32, width: i32) -> i32 {
    let min_px = CardSize::Small.pixel_size().min(px);
    (width - HORIZONTAL_GAP - HORIZONTAL_LABEL_MIN_WIDTH).clamp(min_px, px)
}

/// Cards at or below this position load immediately; the rest yield to the
/// main loop first. An upper bound on one screenful at the default card size.
const VISIBLE_THRESHOLD: u32 = 24;

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

    #[derive(CompositeTemplate)]
    #[template(file = "src/app/components/widgets/card/card.blp")]
    pub struct CardWidget {
        #[template_child]
        pub label_box: TemplateChild<gtk::Box>,

        #[template_child]
        pub title_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub subtitle_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub cover: TemplateChild<gtk::Overlay>,

        #[template_child]
        pub cover_image: TemplateChild<gtk::Picture>,

        #[template_child]
        pub playing_indicator: TemplateChild<gtk::Spinner>,

        /// Current pixel size of the card image.
        pub icon_size: Cell<i32>,
        /// Current layout mode.
        pub layout: Cell<CardLayout>,
        /// Spotify ID for the item this card represents (needed for click handling).
        pub card_id: RefCell<String>,
        pub artwork: RefCell<Option<(String, Arc<ApiService>, bool)>>,
        pub decoded_px: Cell<i32>,
    }

    impl Default for CardWidget {
        fn default() -> Self {
            Self {
                label_box: Default::default(),
                title_label: Default::default(),
                subtitle_label: Default::default(),
                cover: Default::default(),
                cover_image: Default::default(),
                playing_indicator: Default::default(),
                icon_size: Cell::new(CardSize::Large.pixel_size()),
                layout: Cell::new(CardLayout::Vertical),
                card_id: Default::default(),
                artwork: Default::default(),
                decoded_px: Cell::new(0),
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
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let px = self.icon_size.get();
            let layout = self.layout.get();

            if orientation == gtk::Orientation::Horizontal {
                return match layout {
                    CardLayout::Horizontal => {
                        let min = horizontal_image_px(px, 0)
                            + HORIZONTAL_GAP
                            + HORIZONTAL_LABEL_MIN_WIDTH;
                        let nat =
                            px + HORIZONTAL_GAP + (HORIZONTAL_LABEL_WIDTH_SCALE * px as f32) as i32;
                        (min, nat, -1, -1)
                    }
                    _ => (px, px, -1, -1),
                };
            }

            // Vertical measurement.
            match layout {
                CardLayout::Horizontal => {
                    let (img, label_w) = if for_size >= 0 {
                        let img = horizontal_image_px(px, for_size);
                        (img, for_size - img - HORIZONTAL_GAP)
                    } else {
                        (px, (HORIZONTAL_LABEL_WIDTH_SCALE * px as f32) as i32)
                    };
                    let (label_min, _, _, _) = self
                        .label_box
                        .measure(gtk::Orientation::Vertical, label_w.max(0));
                    let h = img.max(label_min);
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

            let (img_x, img_y, img_px) = match layout {
                CardLayout::Vertical | CardLayout::ImageOnly => ((width - px) / 2, 0, px),
                CardLayout::Horizontal => {
                    let px = horizontal_image_px(px, width);
                    (0, (height - px) / 2, px)
                }
            };
            let transform = gtk::gsk::Transform::new()
                .translate(&gtk::graphene::Point::new(img_x as f32, img_y as f32));
            self.cover.allocate(img_px, img_px, -1, Some(transform));

            match layout {
                CardLayout::Vertical => {
                    let label_h = height - px - LABEL_GAP;
                    if label_h > 0 {
                        let transform = gtk::gsk::Transform::new().translate(
                            &gtk::graphene::Point::new(img_x as f32, (px + LABEL_GAP) as f32),
                        );
                        self.label_box.allocate(px, label_h, -1, Some(transform));
                    }
                }
                CardLayout::ImageOnly => {}
                CardLayout::Horizontal => {
                    let px = img_px;
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
    pub struct CardWidget(ObjectSubclass<imp::CardWidget>) @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
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
        api_service: Arc<ApiService>,
        shape: ImageShape,
        layout: CardLayout,
        size: CardSize,
    ) -> Self {
        let widget = Self::new(shape, layout);
        widget.set_image_size(size);
        widget.set_layout(layout);
        widget.bind(model, api_service);
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

        let imp = self.imp();
        if decode_px(size.pixel_size()) > imp.decoded_px.get() {
            let artwork = imp.artwork.borrow().clone();
            if let Some((url, api_service, is_visible)) = artwork {
                let tag = if is_visible {
                    load::visible()
                } else {
                    load::offscreen()
                };
                let load = self.load_artwork(url, api_service, tag);
                glib::MainContext::default()
                    .spawn_local_with_priority(glib::Priority::DEFAULT_IDLE, load);
            }
        }
    }

    fn load_artwork(
        &self,
        url: String,
        api_service: Arc<ApiService>,
        tag: riff_api::Load,
    ) -> impl std::future::Future<Output = ()> + 'static {
        let decode_size = decode_px(self.imp().icon_size.get());
        self.imp().decoded_px.set(decode_size);
        let weak = self.downgrade();
        async move {
            let texture = api_service
                .load_image(&url, decode_size, decode_size, tag)
                .await;
            if let (Some(this), Some(texture)) = (weak.upgrade(), texture) {
                if this.imp().decoded_px.get() == decode_size {
                    this.imp().cover_image.set_paintable(Some(&texture));
                }
            }
        }
    }

    pub fn set_playing(&self, playing: Option<bool>) {
        self.imp().playing_indicator.set_visible(playing.is_some());
        set_css_class(self, "card--paused", playing == Some(false));
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
    fn bind(&self, model: &CardModel, api_service: Arc<ApiService>) {
        let imp = self.imp();
        *imp.card_id.borrow_mut() = model.id();

        // Placeholder cards (empty id) stay in skeleton state.
        if model.id().is_empty() {
            return;
        }

        if let Some(url) = model.image() {
            let title = model.title();
            let subtitle = model.subtitle();
            let position = model.insertion_position();
            let is_visible = position <= VISIBLE_THRESHOLD as i64;
            // Captured now, since the off-screen branch defers to an idle
            // callback by which point the user may have navigated away.
            let tag = if is_visible {
                load::visible()
            } else {
                load::offscreen()
            };

            *imp.artwork.borrow_mut() = Some((url.clone(), api_service.clone(), is_visible));
            let artwork = self.load_artwork(url, api_service, tag);
            let weak = self.downgrade();
            let load = async move {
                artwork.await;
                if let Some(this) = weak.upgrade() {
                    this.imp().title_label.set_label(&title);
                    this.imp().subtitle_label.set_label(&subtitle);
                    this.imp().subtitle_label.set_visible(!subtitle.is_empty());
                    this.set_tooltip_text(Some(&title));
                    this.set_loaded();
                }
            };

            // DEFAULT_IDLE sits below GTK's redraw priority, so artwork never
            // delays a frame; `Priority::DEFAULT` would sit above it.
            let ctx = glib::MainContext::default();
            if is_visible {
                ctx.spawn_local_with_priority(glib::Priority::DEFAULT_IDLE, load);
            } else {
                // Yield once more so a long list doesn't front-load the queue.
                glib::idle_add_local_once(move || {
                    ctx.spawn_local_with_priority(glib::Priority::DEFAULT_IDLE, load);
                });
            }
        }
    }
}
