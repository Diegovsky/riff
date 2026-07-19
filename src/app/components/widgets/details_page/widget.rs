use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use libadwaita::prelude::*;

use crate::app::components::{
    display_add_css_provider, EventListener, HeaderBarComponent, HeaderBarModel, HeaderBarWidget,
    CLAMP_MAX_SIZE,
};
use crate::app::dispatch::Worker;
use crate::app::loader::ImageLoader;
use crate::app::models::ImageSet;

use super::{DetailsHeader, HeaderImageShape, HEADER_IMAGE_SIZE};

// DetailsPage

/// A reusable details page layout used by album, artist, and playlist views.
///
/// Structure (top to bottom):
///   ┌─────────────────────────────┐
///   │ HeaderBarWidget (flat)      │  ← shows title when header scrolls away
///   ├─────────────────────────────┤
///   │ ScrolledWindow              │
///   │  └─ Box (vertical)          │
///   │      ├─ Header (artwork)    │  ← scrolls naturally with content
///   │      └─ Content (tracks)    │
///   └─────────────────────────────┘
pub struct DetailsPage {
    widget: libadwaita::Bin,
    scrolled_window: gtk::ScrolledWindow,
    scroll_child: gtk::Box,
    header_area: gtk::Widget,
    headerbar: Option<HeaderBarWidget>,
    header: DetailsHeader,
}

impl DetailsPage {
    fn load_css() {
        display_add_css_provider(resource!("/components/details_page/style.css"));
    }

    /// Build a new details page.
    ///
    /// - `shape`: controls whether the header artwork is square (albums) or circular (artists).
    /// - `content`: the main body widget (e.g. a track list) placed below the header.
    pub fn new(shape: HeaderImageShape, content: &impl IsA<gtk::Widget>) -> Self {
        Self::load_css();

        // --- Headerbar (top bar that shows title when header is scrolled out of view) ---
        let headerbar = HeaderBarWidget::new();
        headerbar.add_classes(&["details__headerbar"]);

        // --- Header (artwork + title + action buttons) ---
        let header = DetailsHeader::new(shape);
        header.widget().add_css_class("details-header");
        header.widget().set_hexpand(true);
        // BreakpointBin inside the header doesn't propagate child size requests,
        // so we set a minimum height to ensure the header is visible in the ScrolledWindow.
        header.widget().set_size_request(-1, HEADER_IMAGE_SIZE);

        let header_clamp = libadwaita::Clamp::new();
        header_clamp.set_maximum_size(CLAMP_MAX_SIZE);
        header_clamp.set_tightening_threshold(CLAMP_MAX_SIZE);
        header_clamp.set_child(Some(header.widget()));
        header_clamp.add_css_class("details-header-clamp");

        // WindowHandle allows dragging the window from the header area.
        let window_handle = gtk::WindowHandle::new();
        window_handle.set_child(Some(&header_clamp));

        // --- Content (caller-provided body, e.g. track list) ---
        content.upcast_ref::<gtk::Widget>().set_hexpand(true);

        let content_clamp = libadwaita::Clamp::new();
        content_clamp.set_maximum_size(CLAMP_MAX_SIZE);
        content_clamp.set_tightening_threshold(CLAMP_MAX_SIZE);
        content_clamp.set_child(Some(content));
        content_clamp.add_css_class("details-content-clamp");

        // --- Scroll child: vertical box with header + content ---
        let scroll_child = gtk::Box::new(gtk::Orientation::Vertical, 0);
        scroll_child.append(&window_handle);
        scroll_child.append(&content_clamp);
        scroll_child.add_css_class("details-page");
        scroll_child.add_css_class("details-page-content");

        // --- ScrolledWindow encompassing both header and content ---
        let scrolled_window = gtk::ScrolledWindow::new();
        scrolled_window.set_hscrollbar_policy(gtk::PolicyType::Never);
        scrolled_window.set_hexpand(true);
        scrolled_window.set_vexpand(true);
        scrolled_window.set_child(Some(&scroll_child));
        scrolled_window.add_css_class("details-page-scroll");

        // --- Assemble the page ---
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_vexpand(true);
        vbox.set_hexpand(true);
        vbox.append(headerbar.upcast_ref::<gtk::Widget>());
        vbox.append(&scrolled_window);

        let bin = libadwaita::Bin::new();
        bin.set_child(Some(&vbox));

        let header_area: gtk::Widget = window_handle.upcast();

        let page = Self {
            widget: bin,
            scrolled_window,
            scroll_child,
            header_area,
            headerbar: Some(headerbar),
            header,
        };
        page.connect_header_collapse();
        page
    }

    // Accessors

    pub fn widget(&self) -> &libadwaita::Bin {
        &self.widget
    }

    pub fn header(&self) -> &DetailsHeader {
        &self.header
    }

    pub fn headerbar(&self) -> Option<&HeaderBarWidget> {
        self.headerbar.as_ref()
    }

    /// Create a [`HeaderBarComponent`] bound to this page's headerbar widget.
    /// The returned listener should be added to the page's children.
    pub fn create_headerbar_listener(
        &self,
        model: Rc<impl HeaderBarModel + 'static>,
    ) -> Box<dyn EventListener> {
        Box::new(HeaderBarComponent::new(
            self.headerbar().unwrap().clone(),
            model,
        ))
    }

    // Content updates

    /// Set title and subtitle on both the header widget and the collapsed headerbar.
    pub fn set_details(&self, title: &str, subtitle: &str) {
        self.header.set_title(title);
        self.header.set_subtitle(subtitle);
        if let Some(ref headerbar) = self.headerbar {
            headerbar.set_title_and_subtitle(title, subtitle);
        }
    }

    /// Asynchronously load artwork from an ImageSet, or mark the page as loaded if none.
    pub fn load_artwork_or_finish(&self, art: Option<&ImageSet>, worker: &Worker) {
        if let Some(url) = art.and_then(|s| s.best_for_width(HEADER_IMAGE_SIZE as u32)) {
            let url = url.to_string();
            let weak_header = self.header.widget_weak();
            let weak = self.scroll_child.downgrade();
            worker.send_local_task(async move {
                let pixbuf = ImageLoader::new()
                    .load_remote(&url, "jpg", HEADER_IMAGE_SIZE, HEADER_IMAGE_SIZE)
                    .await;
                if let (Some(scroll_child), Some(ref pixbuf)) = (weak.upgrade(), pixbuf) {
                    if let Some(header) = weak_header.upgrade() {
                        let texture = gdk::Texture::for_pixbuf(pixbuf);
                        header.imp().image.set_paintable(Some(&texture));
                        header
                            .imp()
                            .image_box
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
        self.scroll_child.add_css_class("details-page--loaded");
    }

    // Scroll callbacks

    /// Connect a callback for when the user scrolls to the bottom (used for pagination).
    pub fn connect_bottom_edge<F: Fn() + 'static>(&self, f: F) {
        self.scrolled_window.connect_edge_reached(move |_, pos| {
            if let gtk::PositionType::Bottom = pos {
                f()
            }
        });
    }

    // Internal wiring

    /// When the header scrolls out of view, reveal the title in the headerbar
    /// and remove the "flat" (transparent) style so it gets a solid background.
    fn connect_header_collapse(&self) {
        if let Some(ref headerbar) = self.headerbar {
            headerbar.set_title_visible(false);
            headerbar.add_classes(&["flat"]);

            let header_visible = Rc::new(Cell::new(true));
            let adj = self.scrolled_window.vadjustment();
            let header_area = self.header_area.clone();

            adj.connect_value_changed(clone!(
                #[weak]
                headerbar,
                move |adj| {
                    let (_, header_height, _, _) =
                        header_area.measure(gtk::Orientation::Vertical, -1);
                    let header_height = header_height as f64;
                    let scrolled_past = header_height > 0.0 && adj.value() >= header_height * 0.5;
                    if scrolled_past == header_visible.get() {
                        header_visible.set(!scrolled_past);
                        headerbar.set_title_visible(scrolled_past);
                        if scrolled_past {
                            headerbar.remove_classes(&["flat"]);
                        } else {
                            headerbar.add_classes(&["flat"]);
                        }
                    }
                }
            ));
        }
    }
}
