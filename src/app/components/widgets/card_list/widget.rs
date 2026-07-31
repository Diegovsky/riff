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

/// Gap between image and labels in horizontal card layout (matches CardWidget).
const HORIZONTAL_GAP: i32 = 12;

/// Label width multiplier for horizontal card layout (matches CardWidget).
const HORIZONTAL_LABEL_WIDTH_SCALE: f32 = 1.8;

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
    next_position: Rc<Cell<i64>>,
    /// Lowest insertion position assigned so far. Freshly added (prepended)
    /// items are given positions below this so they rank as the most recently
    /// added under the "Recently Added" sort.
    min_position: Rc<Cell<i64>>,
    /// Maximum number of rows to display. None means unlimited.
    max_rows: Rc<Cell<Option<u32>>>,
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

        let max_rows = Rc::new(Cell::new(None));
        let current_size = Rc::new(Cell::new(CardSize::Large));
        let current_layout = Rc::new(Cell::new(CardLayout::Vertical));
        let current_filter = Rc::new(RefCell::new(String::new()));

        // Re-apply the row limit every frame using the FlowBox's actual
        // laid-out geometry. GTK4 FlowBox doesn't emit a resize signal, and the
        // real column count (after theme padding, spacing rounding, and
        // hexpand distribution) can differ from a pixel-math estimate. Reading
        // the children's allocated positions each frame keeps the row limit
        // correct across resizes, card size/layout changes, and content loads.
        // `set_visible` is a no-op when the value is unchanged, so a steady
        // state does not trigger relayouts.
        let max_rows_for_tick = Rc::clone(&max_rows);
        let current_size_for_tick = Rc::clone(&current_size);
        let current_layout_for_tick = Rc::clone(&current_layout);
        let current_filter_for_tick = Rc::clone(&current_filter);
        flowbox.add_tick_callback(move |fb, _| {
            if let Some(rows) = max_rows_for_tick.get() {
                apply_grid_constraint(
                    fb,
                    rows,
                    current_size_for_tick.get(),
                    current_layout_for_tick.get(),
                    &current_filter_for_tick.borrow(),
                );
            }
            glib::ControlFlow::Continue
        });

        Self {
            flowbox,
            current_layout,
            current_size,
            current_sort: Rc::new(Cell::new(SortOrder::RecentlyAdded)),
            current_filter,
            next_position: Rc::new(Cell::new(0)),
            min_position: Rc::new(Cell::new(0)),
            max_rows,
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
        self.min_position.set(0);
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
            let min_pos = Rc::clone(&self.min_position);
            let current_layout = Rc::clone(&self.current_layout);
            let current_size = Rc::clone(&self.current_size);
            let placeholder_count = Rc::clone(&self.placeholder_count);
            let constraint = Rc::clone(&self.max_rows);
            let current_filter = Rc::clone(&self.current_filter);
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
                        // A single/bulk insert at the front of a non-empty store
                        // (a save/follow/create) is the most recently added
                        // content. Give those cards positions below the current
                        // minimum so they sort to the top under "Recently Added";
                        // the FlowBox sort function orders them by their card
                        // properties for every other sort order on insert.
                        let before = source.n_items().saturating_sub(added);
                        let is_prepend = position == 0 && removed == 0 && before > 0;

                        if is_prepend {
                            let min = min_pos.get();
                            for offset in 0..added {
                                let i = position + offset;
                                if let Some(obj) = source.item(i) {
                                    if let Some(card) = obj.downcast_ref::<CardModel>() {
                                        // Item nearest the top of the store
                                        // (offset 0) is the most recent, so it
                                        // gets the smallest position.
                                        let pos = min - (added as i64 - offset as i64);
                                        card.set_insertion_position(pos);
                                        let child =
                                            create_child(card, &worker_clone, shape, layout, size);
                                        flowbox.insert(&child, -1);
                                    }
                                }
                            }
                            min_pos.set(min - added as i64);
                        } else {
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
                    }

                    // Re-apply the row limit after any change
                    if let Some(rows) = constraint.get() {
                        apply_grid_constraint(
                            &flowbox,
                            rows,
                            size,
                            layout,
                            &current_filter.borrow(),
                        );
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
        self.apply_constraint();
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
        self.apply_constraint();
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

    /// Set the maximum number of rows to display.
    ///
    /// Cards that don't fit within the given number of rows will be hidden.
    /// Pass `None` to remove the limit and show all cards.
    /// The row limit is applied after sorting and category filtering.
    pub fn set_max_rows(&self, max_rows: Option<u32>) {
        self.max_rows.set(max_rows);
        self.apply_constraint();
    }

    /// Re-apply the current row limit, showing/hiding children as needed.
    ///
    /// If no limit is active, all children are made visible.
    pub fn apply_constraint(&self) {
        let Some(rows) = self.max_rows.get() else {
            // No limit - make all children visible
            let mut child = self.flowbox.first_child();
            while let Some(c) = child {
                if let Some(fb_child) = c.downcast_ref::<gtk::FlowBoxChild>() {
                    fb_child.set_visible(true);
                }
                child = c.next_sibling();
            }
            return;
        };

        apply_grid_constraint(
            &self.flowbox,
            rows,
            self.current_size.get(),
            self.current_layout.get(),
            &self.current_filter.borrow(),
        );
    }

    /// Show placeholder skeleton cards (only if FlowBox is empty).
    ///
    /// Inserts slightly more placeholders than strictly needed (one extra row's
    /// worth) so the constrained area is never left under-filled if the column
    /// estimate is low. The per-frame reconcile in the tick callback hides any
    /// overflow once the real geometry is known.
    pub fn show_placeholders(&self) {
        if self.flowbox.first_child().is_none() {
            let layout = self.current_layout.get();
            let size = self.current_size.get();
            let count = self.placeholder_insert_count(size, layout);
            for _ in 0..count {
                let child = create_child_placeholder(layout, size);
                self.flowbox.insert(&child, -1);
            }
            self.placeholder_count.set(count);
            self.apply_constraint();
        }
    }

    /// Compute how many placeholders to insert given the current row limit.
    ///
    /// Returns `PLACEHOLDER_COUNT` when there is no row limit. Otherwise inserts
    /// `(rows + 1) * columns` so a low column estimate never under-fills the
    /// constrained rows; excess placeholders are hidden by the reconcile pass.
    fn placeholder_insert_count(&self, size: CardSize, layout: CardLayout) -> u32 {
        match self.max_rows.get() {
            Some(rows) => (rows + 1) * estimated_columns(&self.flowbox, size, layout),
            None => PLACEHOLDER_COUNT,
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

/// Padding applied to each card container via CSS (`.container { padding: 6px }`).
/// Both left and right padding contribute to the effective width.
const CARD_CONTAINER_PADDING: i32 = 6;

/// Determine the number of columns in the first (top) row from the FlowBox's
/// actual laid-out geometry.
///
/// This reads the allocated Y position of each currently-visible child: all
/// children in the top row share the same Y. Counting them yields the true
/// column count as GTK actually laid it out, which is more reliable than a
/// pixel-math estimate (it accounts for theme padding, spacing rounding, and
/// hexpand distribution).
///
/// The top row is always fully populated (overflow is hidden from the end), so
/// this stays correct even when lower rows are hidden. Returns `None` if the
/// FlowBox hasn't been allocated yet.
fn measured_columns(flowbox: &gtk::FlowBox) -> Option<u32> {
    if flowbox.width() <= 0 {
        return None;
    }

    // The Y position of each child relative to the FlowBox. Children in the top
    // row share the smallest Y. `compute_bounds` returns positions in the
    // target's coordinate space (here, the FlowBox itself).
    let child_top = |fb_child: &gtk::FlowBoxChild| -> Option<i32> {
        fb_child
            .compute_bounds(flowbox)
            .map(|bounds| bounds.y().round() as i32)
    };

    // Find the minimum Y among visible children (the top row).
    let mut min_y = i32::MAX;
    let mut child = flowbox.first_child();
    while let Some(c) = child {
        if let Some(fb_child) = c.downcast_ref::<gtk::FlowBoxChild>() {
            if fb_child.is_visible() {
                if let Some(y) = child_top(fb_child) {
                    if y < min_y {
                        min_y = y;
                    }
                }
            }
        }
        child = c.next_sibling();
    }

    if min_y == i32::MAX {
        return None;
    }

    // Count visible children sitting on the top row.
    let mut count = 0u32;
    let mut child = flowbox.first_child();
    while let Some(c) = child {
        if let Some(fb_child) = c.downcast_ref::<gtk::FlowBoxChild>() {
            if fb_child.is_visible() && child_top(fb_child) == Some(min_y) {
                count += 1;
            }
        }
        child = c.next_sibling();
    }

    (count > 0).then_some(count)
}

/// Compute the effective width a single card occupies in the FlowBox, including
/// the column spacing and CSS container padding. Accounts for horizontal layout
/// being wider than vertical or image-only layouts.
///
/// Used as a fallback for computing columns before the FlowBox is laid out; once
/// allocated, `measured_columns` is preferred.
fn effective_card_width(size: CardSize, layout: CardLayout) -> i32 {
    let px = size.pixel_size();
    let card_content_width = match layout {
        CardLayout::Horizontal => {
            px + HORIZONTAL_GAP + (HORIZONTAL_LABEL_WIDTH_SCALE * px as f32) as i32
        }
        _ => px,
    };
    card_content_width + (CARD_CONTAINER_PADDING * 2) + COLUMN_SPACING as i32
}

/// Estimate the column count from pixel math when actual geometry isn't
/// available yet (FlowBox not laid out).
fn estimated_columns(flowbox: &gtk::FlowBox, size: CardSize, layout: CardLayout) -> u32 {
    let eff_width = effective_card_width(size, layout);
    let allocated_width = flowbox.width();
    if allocated_width > 0 && eff_width > 0 {
        let cols = (allocated_width + COLUMN_SPACING as i32) / eff_width;
        cols.max(1) as u32
    } else {
        flowbox.max_children_per_line()
    }
}

/// Apply a row limit to a FlowBox, hiding children that overflow.
///
/// Prefers the real column count from `measured_columns` (actual laid-out
/// geometry) and falls back to a pixel-math estimate when the FlowBox hasn't
/// been allocated yet. The maximum visible count is `rows * columns`. Both real
/// cards (with a model) and placeholders (without) are counted and hidden when
/// they exceed the limit.
fn apply_grid_constraint(
    flowbox: &gtk::FlowBox,
    max_rows: u32,
    size: CardSize,
    layout: CardLayout,
    filter: &str,
) {
    let cols =
        measured_columns(flowbox).unwrap_or_else(|| estimated_columns(flowbox, size, layout));

    let max_visible = (max_rows * cols) as usize;

    let mut visible_index = 0;
    let mut child = flowbox.first_child();
    while let Some(c) = child {
        if let Some(fb_child) = c.downcast_ref::<gtk::FlowBoxChild>() {
            match get_model(fb_child) {
                Some(card) => {
                    let passes_filter = filter.is_empty() || card.category() == *filter;
                    if passes_filter {
                        fb_child.set_visible(visible_index < max_visible);
                        visible_index += 1;
                    } else {
                        fb_child.set_visible(false);
                    }
                }
                None => {
                    // Placeholder - count it toward the limit too
                    fb_child.set_visible(visible_index < max_visible);
                    visible_index += 1;
                }
            }
        }
        child = c.next_sibling();
    }
}
