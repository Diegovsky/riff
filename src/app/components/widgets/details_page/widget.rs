use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use libadwaita::subclass::prelude::*;

use crate::app::components::display_add_css_provider;
use crate::app::components::utils::{decode_px, set_missing_art};
use crate::app::load;
use crate::app::models::ImageSet;
use riff_api::models::is_resource_url;

use super::{DetailsHeader, DetailsHeaderWidget, HeaderImageShape, HEADER_IMAGE_SIZE};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/details_page.ui")]
    pub struct DetailsPageWidget {
        #[template_child]
        pub scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub scroll_child: TemplateChild<gtk::Box>,
        #[template_child]
        pub header_area: TemplateChild<gtk::WindowHandle>,
        #[template_child]
        pub header: TemplateChild<DetailsHeaderWidget>,
        #[template_child]
        pub content: TemplateChild<gtk::Box>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DetailsPageWidget {
        const NAME: &'static str = "DetailsPageWidget";
        type Type = super::DetailsPageWidget;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DetailsPageWidget {}
    impl WidgetImpl for DetailsPageWidget {}
    impl BinImpl for DetailsPageWidget {}
}

glib::wrapper! {
    pub struct DetailsPageWidget(ObjectSubclass<imp::DetailsPageWidget>)
        @extends gtk::Widget, libadwaita::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

/// A reusable details page layout used by album, artist, and playlist views:
/// the header (artwork, title, actions) above the content, scrolling together
/// (see `details_page.blp`).
///
/// The title is shown by the shared [`AppHeaderBar`](crate::app::components::AppHeaderBar),
/// revealed once the artwork scrolls away via [`Self::connect_title_reveal`].
pub struct DetailsPage {
    widget: DetailsPageWidget,
    header: DetailsHeader,
}

impl DetailsPage {
    fn load_css() {
        display_add_css_provider(resource!("/components/details_page/style.css"));
    }

    /// `shape` makes the header artwork square (albums) or circular (artists).
    pub fn new(shape: HeaderImageShape) -> Self {
        Self::load_css();
        let widget: DetailsPageWidget = glib::Object::new();
        let header = DetailsHeader::for_widget(widget.imp().header.get(), shape);
        Self { widget, header }
    }

    // Accessors

    pub fn widget(&self) -> &DetailsPageWidget {
        &self.widget
    }

    /// Where the page's sections go, top to bottom.
    pub fn content(&self) -> &gtk::Box {
        &self.widget.imp().content
    }

    pub fn header(&self) -> &DetailsHeader {
        &self.header
    }

    // Content updates

    /// Set title and subtitle on the header widget.
    pub fn set_details(&self, title: &str, subtitle: &str) {
        self.header.set_title(title);
        self.header.set_subtitle(subtitle);
    }

    /// Asynchronously load artwork from an ImageSet, or mark the page as loaded if none.
    pub fn load_artwork_or_finish(
        &self,
        art: Option<&ImageSet>,
        api_service: Arc<riff_api::ApiService>,
    ) {
        let url = art.and_then(|s| s.best_for_width(HEADER_IMAGE_SIZE as u32));
        if url.is_some_and(is_resource_url) {
            if let Some(header) = self.header.widget_weak().upgrade() {
                let imp = header.imp();
                imp.image.set_paintable(None::<&gdk::Paintable>);
                set_missing_art(&*imp.image_box, true);
                imp.image_box
                    .remove_css_class("details-header__image-placeholder");
            }
            self.set_loaded();
        } else if let Some(url) = url {
            let url = url.to_string();
            let weak_header = self.header.widget_weak();
            let weak = self.widget.imp().scroll_child.downgrade();
            // Captured before spawning, so it describes the view that opened
            // this page.
            let tag = load::hero();
            let size = decode_px(HEADER_IMAGE_SIZE);
            glib::MainContext::default().spawn_local(async move {
                let texture = api_service.load_image(&url, size, size, tag).await;
                if let (Some(scroll_child), Some(ref texture)) = (weak.upgrade(), texture) {
                    if let Some(header) = weak_header.upgrade() {
                        let imp = header.imp();
                        imp.image.set_paintable(Some(texture));
                        set_missing_art(&*imp.image_box, false);
                        imp.image_box
                            .remove_css_class("details-header__image-placeholder");
                    }
                    scroll_child.add_css_class("details-page--loaded");
                }
            });
        } else {
            self.set_loaded();
        }
    }

    /// Mark the page as loaded (triggers CSS transition out of skeleton/loading state).
    pub fn set_loaded(&self) {
        self.widget
            .imp()
            .scroll_child
            .add_css_class("details-page--loaded");
    }

    // Scroll callbacks

    /// Connect a callback for when the user scrolls to the bottom (used for pagination).
    pub fn connect_bottom_edge<F: Fn() + 'static>(&self, f: F) {
        self.widget
            .imp()
            .scrolled_window
            .connect_edge_reached(move |_, pos| {
                if let gtk::PositionType::Bottom = pos {
                    f()
                }
            });
    }

    // Internal wiring

    /// Reveal `title` in the shared header once the artwork scrolls out of
    /// view, hiding it again at the top.
    ///
    /// Uses opacity, not `visible`: the shared header's `GtkStack` won't switch
    /// to a child whose `visible` is false, so it stays visible but transparent.
    pub fn connect_title_reveal(&self, title: &libadwaita::WindowTitle) {
        title.set_opacity(0.0);

        let title_shown = Rc::new(Cell::new(false));
        let adj = self.widget.imp().scrolled_window.vadjustment();
        let header_area = self.widget.imp().header_area.get();

        adj.connect_value_changed(clone!(
            #[weak]
            title,
            move |adj| {
                let (_, header_height, _, _) = header_area.measure(gtk::Orientation::Vertical, -1);
                let header_height = header_height as f64;
                let scrolled_past = header_height > 0.0 && adj.value() >= header_height * 0.5;
                if scrolled_past != title_shown.get() {
                    title_shown.set(scrolled_past);
                    title.set_opacity(if scrolled_past { 1.0 } else { 0.0 });
                }
            }
        ));
    }

    pub fn connect_scrolled_past<F: Fn(bool) + 'static>(&self, widget: &gtk::Widget, f: F) {
        let past = Rc::new(Cell::new(false));
        let scroll_child = self.widget.imp().scroll_child.get();
        let update = Rc::new(clone!(
            #[weak]
            widget,
            move |adj: &gtk::Adjustment| {
                if widget.height() == 0 {
                    return;
                }
                let bottom = gtk::graphene::Point::new(0.0, widget.height() as f32);
                let Some(point) = widget.compute_point(&scroll_child, &bottom) else {
                    return;
                };
                let scrolled_past = adj.value() >= point.y() as f64;
                if scrolled_past != past.get() {
                    past.set(scrolled_past);
                    f(scrolled_past);
                }
            }
        ));

        let adj = self.widget.imp().scrolled_window.vadjustment();
        let on_value = Rc::clone(&update);
        adj.connect_value_changed(move |adj| on_value(adj));
        adj.connect_changed(move |adj| {
            let update = Rc::clone(&update);
            let adj = adj.clone();
            glib::idle_add_local_once(move || update(&adj));
        });
    }
}
