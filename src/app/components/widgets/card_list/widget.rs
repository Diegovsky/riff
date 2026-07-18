use gio::prelude::*;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::{CardLayout, CardSize, CardWidget, ImageShape, SortOrder};
use crate::app::dispatch::Worker;
use crate::app::models::{CardModel, FilterOption};
use crate::app::ListStore;

// Constants

const MIN_CHILDREN_PER_LINE: u32 = 1;
const MAX_CHILDREN_PER_LINE: u32 = 24;
const ROW_SPACING: u32 = 6;
const COLUMN_SPACING: u32 = 6;
const PLACEHOLDER_COUNT: u32 = 50;

/// Key used to attach a CardModel to a FlowBoxChild via GObject unsafe data.
const MODEL_DATA_KEY: &str = "card-model";

/// Trait that abstracts the data/API layer for a card list.
pub trait CardListModel {
    fn get_store(&self) -> Option<impl Deref<Target = ListStore<CardModel>> + '_>;
    fn load_more(&self);
    fn refresh(&self);
    fn has_items(&self) -> bool;
    fn has_more(&self) -> bool {
        false
    }
    fn open_item(&self, id: String);
    fn image_shape(&self) -> ImageShape;

    /// Returns filter options for this card list. Empty = no filter UI.
    fn filter_options(&self) -> Vec<FilterOption> {
        vec![]
    }
}

/// An embeddable FlowBox-based card grid.
///
/// Manually manages FlowBox children and uses `set_sort_func` + `invalidate_sort`
/// for flash-free reordering (no widget destruction/recreation on sort changes).
pub struct CardList {
    flowbox: gtk::FlowBox,
    current_layout: Rc<Cell<CardLayout>>,
    current_size: Rc<Cell<CardSize>>,
    current_sort: Rc<Cell<SortOrder>>,
    current_filter: Rc<RefCell<String>>,
    next_position: Rc<Cell<u32>>,
    /// Number of placeholder children currently in the FlowBox.
    placeholder_count: Rc<Cell<u32>>,
    /// Signal connection to the source store, disconnected on drop.
    source_signal: Cell<Option<(gio::ListStore, glib::SignalHandlerId)>>,
    /// Signal connection for child activation, disconnected on rebind/drop.
    activation_signal: Cell<Option<glib::SignalHandlerId>>,
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
            current_layout: Rc::new(Cell::new(CardLayout::Vertical)),
            current_size: Rc::new(Cell::new(CardSize::Large)),
            current_sort: Rc::new(Cell::new(SortOrder::RecentlyAdded)),
            current_filter: Rc::new(RefCell::new(String::new())),
            next_position: Rc::new(Cell::new(0)),
            placeholder_count: Rc::new(Cell::new(0)),
            source_signal: Cell::new(None),
            activation_signal: Cell::new(None),
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
        // Clean up any previous bind
        if let Some((store, id)) = self.source_signal.take() {
            store.disconnect(id);
        }
        if let Some(id) = self.activation_signal.take() {
            self.flowbox.disconnect(id);
        }
        self.flowbox.remove_all();
        self.next_position.set(0);
        self.placeholder_count.set(0);

        self.current_layout.set(layout);
        self.current_size.set(size);

        if let Some(store) = model.get_store() {
            let shape = model.image_shape();
            let inner = store.inner().clone();

            // Install sort function
            let sort_ref = Rc::clone(&self.current_sort);
            self.flowbox
                .set_sort_func(move |a, b| sort_children(sort_ref.get(), a, b).into());

            // Install filter function
            let filter_ref = Rc::clone(&self.current_filter);
            self.flowbox.set_filter_func(move |child| {
                let filter = filter_ref.borrow();
                if filter.is_empty() {
                    return true; // "All" - show everything
                }
                match get_model(child) {
                    Some(card) => card.category() == *filter,
                    None => true, // Show placeholders
                }
            });

            // Add existing items
            let mut counter = self.next_position.get().max(1);
            for i in 0..inner.n_items() {
                if let Some(obj) = inner.item(i) {
                    if let Some(card) = obj.downcast_ref::<CardModel>() {
                        card.set_insertion_position(counter);
                        counter += 1;
                        self.add_child(card, &worker, shape, layout, size);
                    }
                }
            }
            self.next_position.set(counter);

            // React to source store changes
            let flowbox_weak = self.flowbox.downgrade();
            let next_pos = Rc::clone(&self.next_position);
            let current_layout = Rc::clone(&self.current_layout);
            let current_size = Rc::clone(&self.current_size);
            let placeholder_count = Rc::clone(&self.placeholder_count);
            let worker_clone = worker.clone();
            let handler_id =
                inner.connect_items_changed(move |source, position, removed, added| {
                    let Some(flowbox) = flowbox_weak.upgrade() else {
                        return;
                    };
                    let layout = current_layout.get();
                    let size = current_size.get();

                    // Remove placeholders when real data arrives
                    if added > 0 && placeholder_count.get() > 0 {
                        remove_placeholder_children(&flowbox, &placeholder_count);
                    }

                    // Handle removals: find children that no longer exist in the source store.
                    if removed > 0 {
                        let source_ids: std::collections::HashSet<String> = (0..source.n_items())
                            .filter_map(|i| source.item(i))
                            .filter_map(|o| o.downcast_ref::<CardModel>().map(|c| c.id()))
                            .collect();
                        let mut child = flowbox.first_child();
                        while let Some(c) = child {
                            let next = c.next_sibling();
                            if let Some(fb_child) = c.downcast_ref::<gtk::FlowBoxChild>() {
                                if let Some(card_model) = get_model(fb_child) {
                                    if !source_ids.contains(&card_model.id()) {
                                        flowbox.remove(fb_child);
                                    }
                                }
                            }
                            child = next;
                        }
                    }

                    // Add new items
                    if added > 0 {
                        let mut ctr = next_pos.get();
                        for i in position..(position + added) {
                            if let Some(obj) = source.item(i) {
                                if let Some(card) = obj.downcast_ref::<CardModel>() {
                                    if card.insertion_position() == 0 {
                                        card.set_insertion_position(ctr);
                                        ctr += 1;
                                    }
                                    let child =
                                        create_child(card, &worker_clone, shape, layout, size);
                                    flowbox.insert(&child, -1);
                                }
                            }
                        }
                        next_pos.set(ctr);
                    }
                });
            self.source_signal.set(Some((inner, handler_id)));

            // Handle activation (clicks)
            let weak_model = Rc::downgrade(model);
            let activation_id = self.flowbox.connect_child_activated(move |_, child| {
                if let Some(card_widget) =
                    child.child().and_then(|w| w.downcast::<CardWidget>().ok())
                {
                    let id = card_widget.card_id();
                    if !id.is_empty() {
                        if let Some(model) = weak_model.upgrade() {
                            model.open_item(id);
                        }
                    }
                }
            });
            self.activation_signal.set(Some(activation_id));
        }
    }

    fn add_child(
        &self,
        card: &CardModel,
        worker: &Worker,
        shape: ImageShape,
        layout: CardLayout,
        size: CardSize,
    ) {
        let child = create_child(card, worker, shape, layout, size);
        self.flowbox.insert(&child, -1);
    }

    pub fn update_size(&self, size: CardSize) {
        self.current_size.set(size);
        let mut child = self.flowbox.first_child();
        while let Some(c) = child {
            if let Some(fb_child) = c.downcast_ref::<gtk::FlowBoxChild>() {
                if let Some(card) = fb_child
                    .child()
                    .and_then(|w| w.downcast::<CardWidget>().ok())
                {
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
                if let Some(card) = fb_child
                    .child()
                    .and_then(|w| w.downcast::<CardWidget>().ok())
                {
                    card.set_layout(layout);
                }
            }
            child = c.next_sibling();
        }
    }

    /// Change the sort order. Reorders existing children in-place (no flash).
    pub fn set_sort(&self, sort: SortOrder) {
        self.current_sort.set(sort);
        self.flowbox.invalidate_sort();
    }

    /// Change the active filter category. Empty string = show all.
    ///
    /// Preserves the FlowBox's current height as a minimum so the page doesn't
    /// jump when fewer items are visible. The height is capped at the viewport
    /// height (the ScrolledWindow's visible area) so it never extends beyond
    /// what's on screen. The constraint is released when the filter is cleared
    /// (showing all items).
    pub fn set_filter(&self, category: &str) {
        if category.is_empty() {
            // Showing all - remove height constraint
            self.flowbox.set_height_request(-1);
        } else {
            // Lock the current height before filtering so it doesn't shrink,
            // but cap at the viewport height to avoid extending past the visible area.
            let current_height = self.flowbox.height();
            if current_height > 0 {
                let viewport_height = self
                    .flowbox
                    .ancestor(gtk::ScrolledWindow::static_type())
                    .and_then(|sw| sw.downcast::<gtk::ScrolledWindow>().ok())
                    .map(|sw| sw.vadjustment().page_size() as i32)
                    .unwrap_or(current_height);
                let locked = current_height.min(viewport_height);
                self.flowbox
                    .set_height_request(self.flowbox.height_request().max(locked));
            }
        }
        *self.current_filter.borrow_mut() = category.to_string();
        self.flowbox.invalidate_filter();
    }

    /// Count the number of visible (non-placeholder) children after filtering.
    ///
    /// FlowBox filtering uses `set_child_visible` rather than `set_visible`,
    /// so we must check `is_child_visible()` to detect filtered-out items.
    pub fn visible_count(&self) -> usize {
        let mut count = 0;
        let mut child = self.flowbox.first_child();
        while let Some(c) = child {
            if let Some(fb_child) = c.downcast_ref::<gtk::FlowBoxChild>() {
                if get_model(fb_child).is_some() && fb_child.is_child_visible() {
                    count += 1;
                }
            }
            child = c.next_sibling();
        }
        count
    }

    /// Show placeholder skeleton cards (only if FlowBox is empty).
    pub fn show_placeholders(&self) {
        if self.flowbox.first_child().is_none() {
            let layout = self.current_layout.get();
            let size = self.current_size.get();
            for _ in 0..PLACEHOLDER_COUNT {
                let child = create_child_placeholder(layout, size);
                self.flowbox.insert(&child, -1);
            }
            self.placeholder_count.set(PLACEHOLDER_COUNT);
        }
    }

    /// Append placeholder skeleton cards for pagination loading.
    pub fn append_placeholders(&self) {
        if self.placeholder_count.get() > 0 {
            return;
        }
        let layout = self.current_layout.get();
        let size = self.current_size.get();
        for _ in 0..PLACEHOLDER_COUNT {
            let child = create_child_placeholder(layout, size);
            self.flowbox.insert(&child, -1);
        }
        self.placeholder_count.set(PLACEHOLDER_COUNT);
    }

    /// Remove placeholder children from the FlowBox.
    pub fn remove_placeholders(&self) {
        remove_placeholder_children(&self.flowbox, &self.placeholder_count);
    }
}

impl Drop for CardList {
    fn drop(&mut self) {
        if let Some((store, id)) = self.source_signal.take() {
            store.disconnect(id);
        }
        if let Some(id) = self.activation_signal.take() {
            self.flowbox.disconnect(id);
        }
    }
}

/// Create a FlowBoxChild for a real card model.
fn create_child(
    card: &CardModel,
    worker: &Worker,
    shape: ImageShape,
    layout: CardLayout,
    size: CardSize,
) -> gtk::FlowBoxChild {
    let widget = CardWidget::for_model(card, worker.clone(), shape, layout, size);
    let child = gtk::FlowBoxChild::new();
    child.set_halign(gtk::Align::Fill);
    child.set_hexpand(true);
    child.set_child(Some(&widget));
    // Attach the CardModel so the sort function can access it.
    // SAFETY: MODEL_DATA_KEY is unique to this module and always stores a CardModel.
    // The data outlives the child because GObject prevents use-after-free on qdata.
    unsafe {
        child.set_data(MODEL_DATA_KEY, card.clone());
    }
    child
}

/// Create a FlowBoxChild for a placeholder (skeleton) card.
fn create_child_placeholder(layout: CardLayout, size: CardSize) -> gtk::FlowBoxChild {
    let widget = CardWidget::new(ImageShape::Square, layout);
    widget.set_image_size(size);
    widget.set_layout(layout);
    let child = gtk::FlowBoxChild::new();
    child.set_halign(gtk::Align::Fill);
    child.set_hexpand(true);
    child.set_child(Some(&widget));
    // No model attached - get_model() returning None identifies this as a placeholder.
    child
}

/// Retrieve the attached CardModel from a FlowBoxChild.
fn get_model(child: &gtk::FlowBoxChild) -> Option<CardModel> {
    // SAFETY: Only create_child stores data at MODEL_DATA_KEY, and it always stores CardModel.
    unsafe {
        child
            .data::<CardModel>(MODEL_DATA_KEY)
            .map(|p| p.as_ref().clone())
    }
}

/// Sort function for the FlowBox. Reads CardModel from each child.
/// Children without a model (placeholders) sort to the end.
fn sort_children(sort: SortOrder, a: &gtk::FlowBoxChild, b: &gtk::FlowBoxChild) -> Ordering {
    let a_model = get_model(a);
    let b_model = get_model(b);
    match (a_model.as_ref(), b_model.as_ref()) {
        (Some(a), Some(b)) => compare_cards(sort, a, b),
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
    }
}

/// Remove all placeholder children from the FlowBox.
/// Placeholders are identified by having no attached CardModel.
fn remove_placeholder_children(flowbox: &gtk::FlowBox, count: &Rc<Cell<u32>>) {
    let mut child = flowbox.first_child();
    while let Some(c) = child {
        let next = c.next_sibling();
        if let Some(fb_child) = c.downcast_ref::<gtk::FlowBoxChild>() {
            if get_model(fb_child).is_none() {
                flowbox.remove(fb_child);
            }
        }
        child = next;
    }
    count.set(0);
}

/// Compare two CardModels for sorting.
fn compare_cards(sort: SortOrder, a: &CardModel, b: &CardModel) -> Ordering {
    match sort {
        SortOrder::RecentlyAdded => a.insertion_position().cmp(&b.insertion_position()),
        SortOrder::Alphabetic => a
            .title()
            .to_ascii_lowercase()
            .cmp(&b.title().to_ascii_lowercase()),
        SortOrder::Creator => a
            .subtitle()
            .to_ascii_lowercase()
            .cmp(&b.subtitle().to_ascii_lowercase()),
        SortOrder::DateReleased => b.release_date().cmp(&a.release_date()),
        SortOrder::Popularity => b.popularity().cmp(&a.popularity()),
    }
}
