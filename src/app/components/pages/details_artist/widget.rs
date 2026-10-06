// Widget for the artist detail page.
// Shows a circular artist photo, top tracks as a playlist, and album
// releases as a card grid.

use gettextrs::gettext;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use super::ArtistDetailsModel;

use crate::app::components::{
    CardLayout, CardSize, Component, DetailsPageComponent, EventListener, HasHeaderBarModel,
    HeaderRegistrar, SortOrder,
};
use crate::app::{AppEvent, Dispatcher};

/// GTK widget for the artist detail page.
pub struct ArtistDetails {
    component: DetailsPageComponent<ArtistDetailsModel>,
}

impl ArtistDetails {
    pub fn new(
        model: Rc<ArtistDetailsModel>,
        shared_layout: Rc<Cell<CardLayout>>,
        shared_size: Rc<Cell<CardSize>>,
        dispatcher: Dispatcher,
        registrar: HeaderRegistrar,
        name: String,
    ) -> Self {
        let mut component =
            DetailsPageComponent::new(model.clone(), model.to_headerbar_model(), registrar, name);
        component.create_track_list(Some(&gettext("Top Tracks")));
        let releases = component.create_embedded_card_list(
            Some(&gettext("Releases")),
            "artist_releases",
            &[SortOrder::DateReleased, SortOrder::Alphabetic],
            shared_layout,
            shared_size,
            dispatcher,
        );
        align_to_edges(&releases);

        Self { component }
    }
}

const MIN_COLUMN_SPACING: i32 = 6;

fn align_to_edges(grid: &gtk::FlowBox) {
    grid.add_css_class("artist-releases");
    grid.add_tick_callback(|grid, _| {
        let spacing = edge_to_edge_spacing(grid).unwrap_or(MIN_COLUMN_SPACING);
        if grid.column_spacing() as i32 != spacing {
            grid.set_column_spacing(spacing as u32);
        }
        gtk::glib::ControlFlow::Continue
    });
}

fn edge_to_edge_spacing(grid: &gtk::FlowBox) -> Option<i32> {
    let width = grid.width();
    let (cell_min, cell_nat, _, _) = grid
        .child_at_index(0)?
        .measure(gtk::Orientation::Horizontal, -1);
    if width <= 0 || cell_min != cell_nat || cell_nat <= 0 {
        return None;
    }
    let columns = ((width + MIN_COLUMN_SPACING) / (cell_nat + MIN_COLUMN_SPACING))
        .min(grid.max_children_per_line() as i32);
    if columns < 2 {
        return None;
    }
    Some((width - columns * cell_nat) / (columns - 1))
}

impl Component for ArtistDetails {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.component.get_root_widget()
    }
    fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
        self.component.get_children()
    }
}

impl EventListener for ArtistDetails {
    fn on_event(&mut self, event: &AppEvent) {
        self.component.handle_event(event);
        self.broadcast_event(event);
    }
}
