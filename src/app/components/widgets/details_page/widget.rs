use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use libadwaita::prelude::*;

use crate::app::components::{display_add_css_provider, CLAMP_MAX_SIZE};
use crate::app::dispatch::Worker;
use crate::app::loader::ImageLoader;
use crate::app::models::ImageSet;

use super::{DetailsHeader, HeaderImageShape, HEADER_IMAGE_SIZE};

// DetailsPage

/// A reusable details page layout used by album, artist, and playlist views.
///
/// The title is shown by the shared [`AppHeaderBar`](crate::app::components::AppHeaderBar),
/// revealed once the artwork scrolls away via [`Self::connect_title_reveal`].
///
/// Structure (top to bottom):
///   ┌─────────────────────────────┐
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

        // --- Header (artwork + title + action buttons) ---
        let header = DetailsHeader::new(shape);
        header.widget().add_css_class("details-header");
        header.widget().set_hexpand(true);
        // The header widget reports its own natural height (see
        // DetailsHeaderWidget::measure), so no height request is needed.

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
        let bin = libadwaita::Bin::new();
        bin.set_child(Some(&scrolled_window));

        let header_area: gtk::Widget = window_handle.upcast();

        Self {
            widget: bin,
            scrolled_window,
            scroll_child,
            header_area,
            header,
        }
    }

    // Accessors

    pub fn widget(&self) -> &libadwaita::Bin {
        &self.widget
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
                        let imp = header.imp();
                        imp.image.set_paintable(Some(&texture));
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

    /// Reveal `title` in the shared header once the artwork scrolls out of
    /// view, hiding it again at the top.
    ///
    /// Uses opacity, not `visible`: the shared header's `GtkStack` won't switch
    /// to a child whose `visible` is false, so it stays visible but transparent.
    pub fn connect_title_reveal(&self, title: &libadwaita::WindowTitle) {
        title.set_opacity(0.0);

        let title_shown = Rc::new(Cell::new(false));
        let adj = self.scrolled_window.vadjustment();
        let header_area = self.header_area.clone();

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
}
