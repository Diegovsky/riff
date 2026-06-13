use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use super::card_view_menu::{effective_sort, CardViewMenu};
use super::traits::CardListPageModel;
use super::widget::CardList;
use crate::app::components::{CardLayout, CardSize, Component, EventListener, SortOrder};
use crate::app::dispatch::Worker;
use crate::app::state::LoginEvent;
use crate::app::{ActionDispatcher, AppEvent, BrowserEvent};
use crate::settings::StateTracker;

// Constants

/// Margin (in pixels) around the card list inside the scrolled window.
/// Reduced from 24px to 12px to give more space to cards at smaller sizes.
const CARD_LIST_MARGIN: i32 = 12;

/// Minimum content width (in pixels) for the scrolled window.
const MIN_CONTENT_WIDTH: i32 = 250;

/// A unified card list component that handles events and wiring automatically.
///
/// Analogous to `DetailsPageComponent` for detail pages. Owns the `CardList`,
/// `CardViewMenu`, empty state, and handles `CardStyleChanged` events internally.
/// Sort is handled locally by the `CardViewMenu` (no round-trip through app state).
pub struct CardListComponent<M: CardListPageModel + 'static> {
    model: Rc<M>,
    widget: gtk::Box,
    status_page: libadwaita::StatusPage,
    scrolled_window: gtk::ScrolledWindow,
    card_list: Rc<CardList>,
    view_menu: CardViewMenu,
    layout: Rc<Cell<CardLayout>>,
    size: Rc<Cell<CardSize>>,
    current_sort: Rc<Cell<SortOrder>>,
}

impl<M: CardListPageModel + 'static> CardListComponent<M> {
    pub fn new(
        model: Rc<M>,
        worker: Worker,
        layout: Rc<Cell<CardLayout>>,
        size: Rc<Cell<CardSize>>,
        dispatcher: Box<dyn ActionDispatcher>,
    ) -> Self {
        let card_list = Rc::new(CardList::new());
        card_list.widget().set_margin_start(CARD_LIST_MARGIN);
        card_list.widget().set_margin_end(CARD_LIST_MARGIN);
        card_list.widget().set_margin_top(CARD_LIST_MARGIN);
        card_list.widget().set_margin_bottom(CARD_LIST_MARGIN);

        let status_page = libadwaita::StatusPage::new();
        status_page.set_title(&model.empty_title());
        status_page.set_description(Some(&model.empty_description()));
        status_page.set_icon_name(Some(model.empty_icon()));
        status_page.set_visible(true);

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(card_list.widget()));
        overlay.add_overlay(&status_page);

        let scrolled_window = gtk::ScrolledWindow::new();
        scrolled_window.set_hexpand(true);
        scrolled_window.set_vexpand(true);
        scrolled_window.set_min_content_width(MIN_CONTENT_WIDTH);
        scrolled_window.set_child(Some(&overlay));

        scrolled_window.connect_edge_reached(clone!(
            #[weak]
            model,
            move |_, pos| {
                if pos == gtk::PositionType::Bottom {
                    model.load_more();
                }
            }
        ));

        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.add_css_class("saved-items-page");
        widget.append(&scrolled_window);

        card_list.bind(&model, worker, layout.get(), size.get());

        let page_id = model.page_id().to_string();
        let preferred_sort = StateTracker::load_sort_order(&page_id);
        let current_sort = effective_sort(preferred_sort, model.available_sort_orders());
        let current_sort = Rc::new(Cell::new(current_sort));

        // Apply initial sort if not default
        if current_sort.get() != SortOrder::RecentlyAdded {
            card_list.set_sort(current_sort.get());
        }

        let view_menu = CardViewMenu::new(
            page_id,
            model.available_sort_orders(),
            Rc::clone(&layout),
            Rc::clone(&size),
            Rc::clone(&current_sort),
            Rc::clone(&card_list),
            dispatcher,
        );

        Self {
            model,
            widget,
            status_page,
            scrolled_window,
            card_list,
            view_menu,
            layout,
            size,
            current_sort,
        }
    }

    pub fn view_button(&self) -> &libadwaita::SplitButton {
        self.view_menu.widget()
    }
}

impl<M: CardListPageModel + 'static> EventListener for CardListComponent<M> {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::LoginEvent(LoginEvent::LoginCompleted) => {
                self.model.refresh();
            }
            AppEvent::BrowserEvent(BrowserEvent::CardStyleChanged(_, _)) => {
                self.card_list.update_layout(self.layout.get());
                self.card_list.update_size(self.size.get());
                self.view_menu.sync(self.layout.get());
            }
            _ => {}
        }

        if self.model.should_refresh(event) {
            self.status_page.set_visible(!self.model.has_items());
            let adj = self.scrolled_window.vadjustment();
            if adj.upper() <= adj.page_size() {
                self.model.load_more();
            }
            // Re-apply sort after data changes
            let sort = self.current_sort.get();
            if sort != SortOrder::RecentlyAdded {
                self.card_list.set_sort(sort);
            }
        }
    }
}

impl<M: CardListPageModel + 'static> Component for CardListComponent<M> {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.widget.as_ref()
    }
}

/// A lightweight card list handler for embedding inside a `DetailsPageComponent`.
///
/// Unlike `CardListComponent`, this does NOT own a scrolled window or status page
/// (the parent detail page provides those). It only manages the `CardList` +
/// `CardViewMenu` + style event handling. Sort is handled locally by the menu.
pub struct EmbeddedCardList {
    card_list: Rc<CardList>,
    view_menu: CardViewMenu,
    layout: Rc<Cell<CardLayout>>,
    size: Rc<Cell<CardSize>>,
}

impl EmbeddedCardList {
    pub fn new(
        card_list: Rc<CardList>,
        page_id: &str,
        available_sorts: &[SortOrder],
        layout: Rc<Cell<CardLayout>>,
        size: Rc<Cell<CardSize>>,
        dispatcher: Box<dyn ActionDispatcher>,
    ) -> Self {
        // Apply shared style/size (card list may have been created with defaults)
        card_list.update_layout(layout.get());
        card_list.update_size(size.get());

        let preferred_sort = StateTracker::load_sort_order(page_id);
        let current_sort = Rc::new(Cell::new(effective_sort(preferred_sort, available_sorts)));

        // Apply initial sort if not default
        if current_sort.get() != SortOrder::RecentlyAdded {
            card_list.set_sort(current_sort.get());
        }

        let view_menu = CardViewMenu::new(
            page_id.to_string(),
            available_sorts,
            Rc::clone(&layout),
            Rc::clone(&size),
            current_sort,
            Rc::clone(&card_list),
            dispatcher,
        );

        Self {
            card_list,
            view_menu,
            layout,
            size,
        }
    }

    pub fn view_button(&self) -> &libadwaita::SplitButton {
        self.view_menu.widget()
    }
}

impl EventListener for EmbeddedCardList {
    fn on_event(&mut self, event: &AppEvent) {
        if let AppEvent::BrowserEvent(BrowserEvent::CardStyleChanged(_, _)) = event {
            self.card_list.update_layout(self.layout.get());
            self.card_list.update_size(self.size.get());
            self.view_menu.sync(self.layout.get());
        }
    }
}
