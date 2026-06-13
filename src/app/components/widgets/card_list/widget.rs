use gio::prelude::*;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::utils::wrap_flowbox_item;
use crate::app::components::{CardLayout, CardSize, CardWidget, ImageShape, SortOrder};
use crate::app::dispatch::Worker;
use crate::app::models::CardModel;
use crate::app::ListStore;

// Constants

/// Minimum number of cards per row in the flow box.
const MIN_CHILDREN_PER_LINE: u32 = 1;

/// Maximum number of cards per row in the flow box.
const MAX_CHILDREN_PER_LINE: u32 = 24;

/// Spacing (in pixels) between rows in the flow box.
const ROW_SPACING: u32 = 6;

/// Spacing (in pixels) between columns in the flow box.
const COLUMN_SPACING: u32 = 6;

/// Trait that abstracts the data/API layer for a card list.
pub trait CardListModel {
    fn get_store(&self) -> Option<impl Deref<Target = ListStore<CardModel>> + '_>;
    fn load_more(&self);
    fn refresh(&self);
    fn has_items(&self) -> bool;
    fn open_item(&self, id: String);
    fn image_shape(&self) -> ImageShape;
}

/// An embeddable FlowBox-based card grid.
///
/// Sorting is performed directly on the underlying `gio::ListStore`, which
/// causes the bound FlowBox to re-render in the new order automatically.
pub struct CardList {
    flowbox: gtk::FlowBox,
    store: Rc<RefCell<Option<gio::ListStore>>>,
    current_layout: Rc<Cell<CardLayout>>,
    current_size: Rc<Cell<CardSize>>,
    /// Monotonic counter for stamping insertion order (immune to sort reordering).
    next_position: Rc<Cell<u32>>,
}

impl CardList {
    pub fn new() -> Self {
        let flowbox = gtk::FlowBox::new();
        flowbox.set_min_children_per_line(MIN_CHILDREN_PER_LINE);
        flowbox.set_max_children_per_line(MAX_CHILDREN_PER_LINE);
        flowbox.set_row_spacing(ROW_SPACING);
        flowbox.set_column_spacing(COLUMN_SPACING);
        flowbox.set_selection_mode(gtk::SelectionMode::None);
        flowbox.set_activate_on_single_click(true);
        flowbox.set_valign(gtk::Align::Start);
        Self {
            flowbox,
            store: Rc::new(RefCell::new(None)),
            current_layout: Rc::new(Cell::new(CardLayout::Vertical)),
            current_size: Rc::new(Cell::new(CardSize::Large)),
            next_position: Rc::new(Cell::new(0)),
        }
    }

    pub fn widget(&self) -> &gtk::FlowBox {
        &self.flowbox
    }

    pub fn bind<M: CardListModel + 'static>(
        &self,
        model: &Rc<M>,
        worker: Worker,
        layout: CardLayout,
        size: CardSize,
    ) {
        self.current_layout.set(layout);
        self.current_size.set(size);

        if let Some(store) = model.get_store() {
            let shape = model.image_shape();
            let inner = store.inner().clone();
            self.store.replace(Some(inner.clone()));

            // Stamp insertion positions on existing items using monotonic counter
            let mut counter = self.next_position.get();
            for i in 0..inner.n_items() {
                if let Some(obj) = inner.item(i) {
                    if let Some(card) = obj.downcast_ref::<CardModel>() {
                        card.set_insertion_position(counter);
                        counter += 1;
                    }
                }
            }
            self.next_position.set(counter);

            // Track new items appended to the store (skip reorders from sort)
            let next_pos = Rc::clone(&self.next_position);
            let store_ref = inner.clone();
            inner.connect_items_changed(move |_, position, removed, added| {
                if removed > 0 {
                    return; // This is a reorder (e.g. from sort), not new data
                }
                let mut counter = next_pos.get();
                for i in position..(position + added) {
                    if let Some(obj) = store_ref.item(i) {
                        if let Some(card) = obj.downcast_ref::<CardModel>() {
                            card.set_insertion_position(counter);
                            counter += 1;
                        }
                    }
                }
                next_pos.set(counter);
            });

            let current_layout = Rc::clone(&self.current_layout);
            let current_size = Rc::clone(&self.current_size);

            self.flowbox.bind_model(Some(&inner), move |item| {
                let layout = current_layout.get();
                let size = current_size.get();
                wrap_flowbox_item(item, |card: &CardModel| {
                    CardWidget::for_model(card, worker.clone(), shape, layout, size)
                })
            });

            let weak_model = Rc::downgrade(model);
            self.flowbox.connect_child_activated(move |_, child| {
                if let Some(card_widget) = child.child().and_then(|w| w.downcast::<CardWidget>().ok()) {
                    let id = card_widget.card_id();
                    if !id.is_empty() {
                        if let Some(model) = weak_model.upgrade() {
                            model.open_item(id);
                        }
                    }
                }
            });
        }
    }

    pub fn update_size(&self, size: CardSize) {
        self.current_size.set(size);
        let mut child = self.flowbox.first_child();
        while let Some(c) = child {
            if let Some(fb_child) = c.downcast_ref::<gtk::FlowBoxChild>() {
                if let Some(card) = fb_child.child().and_then(|w| w.downcast::<CardWidget>().ok()) {
                    card.set_image_size(size);
                }
            }
            child = c.next_sibling();
        }
    }

    pub fn update_layout(&self, layout: CardLayout) {
        self.current_layout.set(layout);
        let mut child = self.flowbox.first_child();
        while let Some(c) = child {
            if let Some(fb_child) = c.downcast_ref::<gtk::FlowBoxChild>() {
                if let Some(card) = fb_child.child().and_then(|w| w.downcast::<CardWidget>().ok()) {
                    card.set_layout(layout);
                }
            }
            child = c.next_sibling();
        }
    }

    /// Sort the underlying ListStore. The FlowBox updates automatically via bind_model.
    pub fn set_sort(&self, sort: SortOrder) {
        if let Some(ref s) = *self.store.borrow() {
            s.sort(|a, b| sort_card_models(sort, a, b));
        }
    }
}

/// Compare two CardModel objects for sorting.
fn sort_card_models(sort: SortOrder, a: &glib::Object, b: &glib::Object) -> Ordering {
    let a = a.downcast_ref::<CardModel>();
    let b = b.downcast_ref::<CardModel>();
    match (a, b) {
        (Some(a), Some(b)) => match sort {
            SortOrder::RecentlyAdded => a.insertion_position().cmp(&b.insertion_position()),
            SortOrder::Alphabetic => a.title().to_lowercase().cmp(&b.title().to_lowercase()),
            SortOrder::Creator => a.subtitle().to_lowercase().cmp(&b.subtitle().to_lowercase()),
            SortOrder::DateReleased => b.release_date().cmp(&a.release_date()), // newest first
            SortOrder::Popularity => b.popularity().cmp(&a.popularity()), // highest first
        },
        _ => Ordering::Equal,
    }
}
