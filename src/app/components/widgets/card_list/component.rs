use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use super::card_view_menu::{effective_sort, CardViewMenu};
use super::page_widget::CardListWidget;
use super::traits::CardListPageModel;
use super::widget::CardList;
use crate::app::components::{CardLayout, CardSize, Component, EventListener, SortOrder};
use crate::app::dispatch::Worker;
use crate::app::state::LoginEvent;
use crate::app::{ActionDispatcher, AppEvent, BrowserEvent};
use crate::settings::StateTracker;

// Constants

/// Margin (in pixels) around the card list inside the scrolled window.
const CARD_LIST_MARGIN: i32 = 12;

/// A unified card list component that handles events and wiring automatically.
///
/// Owns the `CardListWidget` (template-based), `CardList`, `CardViewMenu`,
/// and handles `CardLayoutChanged`/`CardSizeChanged` events internally.
pub struct CardListComponent<M: CardListPageModel + 'static> {
    model: Rc<M>,
    page_widget: CardListWidget,
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
        dispatcher: Rc<dyn ActionDispatcher>,
    ) -> Self {
        let page_widget = CardListWidget::new();

        let card_list = Rc::new(CardList::new());
        card_list.widget().set_margin_start(CARD_LIST_MARGIN);
        card_list.widget().set_margin_end(CARD_LIST_MARGIN);
        card_list.widget().set_margin_top(CARD_LIST_MARGIN);
        card_list.widget().set_margin_bottom(CARD_LIST_MARGIN);

        // Wire card list into template overlay
        page_widget.overlay().set_child(Some(card_list.widget()));

        // Configure status page from model
        let status_page = page_widget.status_page();
        status_page.set_title(&model.empty_title());
        status_page.set_description(Some(&model.empty_description()));
        status_page.set_icon_name(Some(model.empty_icon()));
        status_page.set_visible(false);

        // Infinite scroll
        let card_list_weak = Rc::downgrade(&card_list);
        page_widget.scrolled_window().connect_edge_reached(clone!(
            #[weak]
            model,
            move |_, pos| {
                if pos == gtk::PositionType::Bottom && model.has_more() {
                    if let Some(cl) = card_list_weak.upgrade() {
                        cl.append_placeholders();
                    }
                    model.load_more();
                }
            }
        ));

        card_list.bind(&model, worker, layout.get(), size.get());
        card_list.show_placeholders();

        let page_id = model.page_id().to_string();
        let tracker = StateTracker::new_from_gsettings();
        let preferred_sort = tracker.load_sort_order(&page_id);
        let current_sort = effective_sort(preferred_sort, model.available_sort_orders());
        let current_sort = Rc::new(Cell::new(current_sort));

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
            page_widget,
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
                self.card_list.show_placeholders();
                self.model.refresh();
            }
            AppEvent::LoginEvent(LoginEvent::LogoutCompleted) => {
                self.card_list.widget().remove_all();
                self.page_widget.status_page().set_visible(false);
            }
            AppEvent::BrowserEvent(
                BrowserEvent::CardLayoutChanged(_) | BrowserEvent::CardSizeChanged(_),
            ) => {
                self.card_list.update_layout(self.layout.get());
                self.card_list.update_size(self.size.get());
                self.view_menu.sync(self.layout.get());
            }
            _ => {}
        }

        if self.model.should_refresh(event) {
            self.card_list.remove_placeholders();
            self.page_widget
                .status_page()
                .set_visible(!self.model.has_items());
            if self.model.has_items() {
                let adj = self.page_widget.scrolled_window().vadjustment();
                if adj.upper() <= adj.page_size() && self.model.has_more() {
                    self.card_list.append_placeholders();
                    self.model.load_more();
                }
                let sort = self.current_sort.get();
                if sort != SortOrder::RecentlyAdded {
                    self.card_list.set_sort(sort);
                }
            }
        }
    }
}

impl<M: CardListPageModel + 'static> Component for CardListComponent<M> {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.page_widget.as_ref()
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
        dispatcher: Rc<dyn ActionDispatcher>,
    ) -> Self {
        // Apply shared style/size (card list may have been created with defaults)
        card_list.update_layout(layout.get());
        card_list.update_size(size.get());

        let tracker = StateTracker::new_from_gsettings();
        let preferred_sort = tracker.load_sort_order(page_id);
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
        if let AppEvent::BrowserEvent(
            BrowserEvent::CardLayoutChanged(_) | BrowserEvent::CardSizeChanged(_),
        ) = event
        {
            self.card_list.update_layout(self.layout.get());
            self.card_list.update_size(self.size.get());
            self.view_menu.sync(self.layout.get());
        }
    }
}
