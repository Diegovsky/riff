// Reusable card widget (artwork + title + subtitle) used in grid layouts.
// Defined via a Blueprint template (card.blp) and styled with card.css.

use crate::app::components::display_add_css_provider;
use crate::app::dispatch::Worker;
use crate::app::loader::ImageLoader;
use crate::app::models::CardModel;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use libadwaita::subclass::prelude::BinImpl;

/// Controls whether the card artwork is circular or square.
#[derive(Debug, Clone, Copy)]
pub enum ImageShape {
    Round,
    Square,
}

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(file = "src/app/components/widgets/card/card.blp")]
    pub struct CardWidget {
        #[template_child]
        pub title_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub subtitle_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub cover_image: TemplateChild<gtk::Picture>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CardWidget {
        const NAME: &'static str = "CardWidget";
        type Type = super::CardWidget;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for CardWidget {}
    impl WidgetImpl for CardWidget {}
    impl BinImpl for CardWidget {}
}

glib::wrapper! {
    pub struct CardWidget(ObjectSubclass<imp::CardWidget>) @extends gtk::Widget, libadwaita::Bin;
}

impl CardWidget {
    pub fn new(shape: ImageShape) -> Self {
        display_add_css_provider(resource!("/components/card.css"));
        let widget: Self = glib::Object::new();
        widget.add_css_class("container");
        match shape {
            ImageShape::Round => widget.add_css_class("card--round"),
            ImageShape::Square => widget.add_css_class("card--square"),
        }
        widget
    }

    pub fn for_model(model: &CardModel, worker: Worker, shape: ImageShape) -> Self {
        let widget = Self::new(shape);
        widget.bind(model, worker);
        widget
    }

    fn set_loaded(&self) {
        self.add_css_class("container--loaded");
    }

    fn bind(&self, model: &CardModel, worker: Worker) {
        let imp = self.imp();
        imp.cover_image.set_overflow(gtk::Overflow::Hidden);

        if let Some(url) = model.image() {
            let weak = self.downgrade();
            let title = model.title();
            let subtitle = model.subtitle();
            worker.send_local_task(async move {
                if let Some(this) = weak.upgrade() {
                    let loader = ImageLoader::new();
                    let pixbuf = loader.load_remote(&url, "jpg", 200, 200).await;
                    if let Some(pixbuf) = pixbuf.as_ref() {
                        let texture = gdk::Texture::for_pixbuf(pixbuf);
                        this.imp().cover_image.set_paintable(Some(&texture));
                    }
                    this.imp().title_label.set_label(&title);
                    this.imp().subtitle_label.set_label(&subtitle);
                    this.set_loaded();
                }
            });
        } else {
            model
                .bind_property("title", &*imp.title_label, "label")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build();

            model
                .bind_property("subtitle", &*imp.subtitle_label, "label")
                .flags(glib::BindingFlags::DEFAULT | glib::BindingFlags::SYNC_CREATE)
                .build();

            self.set_loaded();
        }
    }
}
