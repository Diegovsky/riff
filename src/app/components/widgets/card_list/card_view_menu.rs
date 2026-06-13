use gettextrs::gettext;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use super::widget::CardList;
use crate::app::components::{CardLayout, CardSize, SortOrder};
use crate::app::{ActionDispatcher, BrowserAction};
use crate::settings::StateTracker;

// Constants

/// Margin (in pixels) around the popover content.
const POPOVER_MARGIN: i32 = 12;

/// Spacing (in pixels) between items in the popover and size button row.
const POPOVER_SPACING: i32 = 6;

/// Margin (in pixels) above and below the separator line.
const SEPARATOR_MARGIN: i32 = 6;

/// Returns the sort to actually apply: the user's preferred sort if it's
/// available for this page, otherwise the first available sort.
pub(super) fn effective_sort(preferred: SortOrder, available: &[SortOrder]) -> SortOrder {
    if available.contains(&preferred) {
        preferred
    } else {
        let fallback = available.first().copied().unwrap_or(SortOrder::RecentlyAdded);
        debug!(
            "Preferred sort {:?} not available, falling back to {:?}",
            preferred, fallback
        );
        fallback
    }
}

fn icon_for_layout(layout: CardLayout) -> &'static str {
    match layout {
        CardLayout::Vertical => "view-grid-symbolic",
        CardLayout::ImageOnly => "view-app-grid-symbolic",
        CardLayout::Horizontal => "view-list-symbolic",
    }
}

/// A Nautilus-style split button: clicking cycles the card layout,
/// the dropdown arrow opens a popover with icon size controls and sort options.
pub struct CardViewMenu {
    pub split_button: libadwaita::SplitButton,
}

impl CardViewMenu {
    pub fn new(
        page_id: String,
        available_sorts: &[SortOrder],
        layout: Rc<Cell<CardLayout>>,
        size: Rc<Cell<CardSize>>,
        current_sort: Rc<Cell<SortOrder>>,
        card_list: Rc<CardList>,
        dispatcher: Box<dyn ActionDispatcher>,
    ) -> Self {
        let popover = Self::build_popover(
            &page_id,
            available_sorts,
            current_sort.get(),
            size.get(),
            Rc::clone(&layout),
            Rc::clone(&size),
            Rc::clone(&current_sort),
            Rc::clone(&card_list),
            dispatcher.box_clone(),
        );

        let split_button = libadwaita::SplitButton::new();
        split_button.set_icon_name(icon_for_layout(layout.get()));
        split_button.set_popover(Some(&popover));

        let layout_ref = Rc::clone(&layout);
        let size_ref = Rc::clone(&size);
        let card_list_ref = Rc::clone(&card_list);
        let dispatch = dispatcher.box_clone();
        split_button.connect_clicked(move |btn| {
            let next = layout_ref.get().next();
            layout_ref.set(next);
            btn.set_icon_name(icon_for_layout(next));
            card_list_ref.update_layout(next);
            dispatch.dispatch(BrowserAction::ChangeCardStyle(next, size_ref.get()).into());
        });

        Self { split_button }
    }

    pub fn widget(&self) -> &libadwaita::SplitButton {
        &self.split_button
    }

    pub fn sync(&self, layout: CardLayout) {
        self.split_button.set_icon_name(icon_for_layout(layout));
    }

    fn build_popover(
        page_id: &str,
        available_sorts: &[SortOrder],
        current_sort: SortOrder,
        current_size: CardSize,
        layout: Rc<Cell<CardLayout>>,
        size: Rc<Cell<CardSize>>,
        sort: Rc<Cell<SortOrder>>,
        card_list: Rc<CardList>,
        dispatcher: Box<dyn ActionDispatcher>,
    ) -> gtk::Popover {
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, POPOVER_SPACING);
        vbox.set_margin_top(POPOVER_MARGIN);
        vbox.set_margin_bottom(POPOVER_MARGIN);
        vbox.set_margin_start(POPOVER_MARGIN);
        vbox.set_margin_end(POPOVER_MARGIN);

        let (size_box, decrease_btn, increase_btn) =
            Self::build_size_section(current_size, layout, Rc::clone(&size), Rc::clone(&card_list), dispatcher);
        vbox.append(&size_box);

        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        separator.set_margin_top(SEPARATOR_MARGIN);
        separator.set_margin_bottom(SEPARATOR_MARGIN);
        vbox.append(&separator);

        Self::build_sort_section(&vbox, page_id, available_sorts, current_sort, sort, card_list);

        let popover = gtk::Popover::new();
        popover.set_child(Some(&vbox));

        // Sync button sensitivity when popover opens (size may have changed on another page)
        let size_ref = size;
        popover.connect_show(move |_| {
            let s = size_ref.get();
            decrease_btn.set_sensitive(s != CardSize::Small);
            increase_btn.set_sensitive(s != CardSize::Large);
        });

        popover
    }

    /// Build the "Icon Size" row with zoom-out / zoom-in buttons.
    fn build_size_section(
        current_size: CardSize,
        layout: Rc<Cell<CardLayout>>,
        size: Rc<Cell<CardSize>>,
        card_list: Rc<CardList>,
        dispatcher: Box<dyn ActionDispatcher>,
    ) -> (gtk::Box, gtk::Button, gtk::Button) {
        let size_box = gtk::Box::new(gtk::Orientation::Horizontal, POPOVER_SPACING);
        let size_label = gtk::Label::new(Some(&gettext("Icon Size")));
        size_label.set_halign(gtk::Align::Start);
        size_label.set_hexpand(true);
        size_label.add_css_class("heading");
        size_box.append(&size_label);

        let decrease_btn = gtk::Button::from_icon_name("zoom-out-symbolic");
        let increase_btn = gtk::Button::from_icon_name("zoom-in-symbolic");
        decrease_btn.add_css_class("flat");
        increase_btn.add_css_class("flat");
        decrease_btn.set_margin_start(POPOVER_MARGIN);
        decrease_btn.set_sensitive(current_size != CardSize::Small);
        increase_btn.set_sensitive(current_size != CardSize::Large);

        size_box.append(&decrease_btn);
        size_box.append(&increase_btn);

        let size_ref = Rc::clone(&size);
        let layout_ref = Rc::clone(&layout);
        let card_list_ref = Rc::clone(&card_list);
        let inc_btn = increase_btn.clone();
        let dispatch = dispatcher.box_clone();
        decrease_btn.connect_clicked(move |btn| {
            let new_size = size_ref.get().decrease();
            size_ref.set(new_size);
            card_list_ref.update_size(new_size);
            btn.set_sensitive(new_size != CardSize::Small);
            inc_btn.set_sensitive(true);
            dispatch.dispatch(BrowserAction::ChangeCardStyle(layout_ref.get(), new_size).into());
        });

        let size_ref = Rc::clone(&size);
        let layout_ref = layout;
        let card_list_ref = card_list;
        let dec_btn = decrease_btn.clone();
        let dispatch = dispatcher.box_clone();
        increase_btn.connect_clicked(move |btn| {
            let new_size = size_ref.get().increase();
            size_ref.set(new_size);
            card_list_ref.update_size(new_size);
            btn.set_sensitive(new_size != CardSize::Large);
            dec_btn.set_sensitive(true);
            dispatch.dispatch(BrowserAction::ChangeCardStyle(layout_ref.get(), new_size).into());
        });

        (size_box, decrease_btn, increase_btn)
    }

    /// Build the "Sort By" radio button group and append to `container`.
    fn build_sort_section(
        container: &gtk::Box,
        page_id: &str,
        available_sorts: &[SortOrder],
        current_sort: SortOrder,
        sort: Rc<Cell<SortOrder>>,
        card_list: Rc<CardList>,
    ) {
        let sort_label = gtk::Label::new(Some(&gettext("Sort By")));
        sort_label.set_halign(gtk::Align::Start);
        sort_label.add_css_class("heading");
        container.append(&sort_label);

        let all_sort_options = [
            SortOrder::RecentlyAdded,
            SortOrder::Alphabetic,
            SortOrder::Creator,
            SortOrder::DateReleased,
            SortOrder::Popularity,
        ];

        let mut first_btn: Option<gtk::CheckButton> = None;
        for order in all_sort_options {
            if !available_sorts.contains(&order) {
                continue;
            }
            let btn = gtk::CheckButton::with_label(&order.label());
            if let Some(ref group) = first_btn {
                btn.set_group(Some(group));
            } else {
                first_btn = Some(btn.clone());
            }
            btn.set_active(order == current_sort);

            let page = page_id.to_string();
            let card_list_ref = Rc::clone(&card_list);
            let sort_ref = Rc::clone(&sort);
            btn.connect_toggled(move |b| {
                if b.is_active() {
                    sort_ref.set(order);
                    card_list_ref.set_sort(order);
                    StateTracker::save_sort_order(&page, order);
                }
            });

            container.append(&btn);
        }
    }
}
