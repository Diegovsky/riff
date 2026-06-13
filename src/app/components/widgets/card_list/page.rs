// Standalone page wrapping a CardList with a ScrolledWindow, bottom-edge
// pagination, and an empty state placeholder. Used for saved albums,
// saved playlists, and saved artists views.

use gtk::prelude::*;
use std::rc::Rc;

use super::widget::{CardList, CardListModel};
use crate::app::components::{Component, EventListener};
use crate::app::dispatch::Worker;
use crate::app::state::LoginEvent;
use crate::app::AppEvent;

/// Configuration for the empty state placeholder shown when no items are loaded.
pub struct CardListPageConfig {
    pub empty_title: String,
    pub empty_description: String,
    pub empty_icon: String,
}

/// A standalone page with ScrolledWindow + StatusPage + CardList.
/// Handles pagination via scroll-to-bottom detection.
pub struct CardListPage<M: CardListModel + 'static> {
    widget: gtk::Box,
    status_page: libadwaita::StatusPage,
    scrolled_window: gtk::ScrolledWindow,
    #[allow(dead_code)]
    card_list: CardList,
    model: Rc<M>,
    update_event: Box<dyn Fn(&AppEvent) -> bool>,
}

impl<M: CardListModel + 'static> CardListPage<M> {
    pub fn new(
        model: M,
        worker: Worker,
        config: CardListPageConfig,
        update_event: impl Fn(&AppEvent) -> bool + 'static,
    ) -> Self {
        let model = Rc::new(model);
        let card_list = CardList::new();
        card_list.widget().set_margin_start(24);
        card_list.widget().set_margin_end(24);
        card_list.widget().set_margin_top(24);
        card_list.widget().set_margin_bottom(24);

        // Build the widget tree: Box > ScrolledWindow > Overlay { FlowBox, StatusPage }
        let status_page = libadwaita::StatusPage::new();
        status_page.set_title(&config.empty_title);
        status_page.set_description(Some(&config.empty_description));
        status_page.set_icon_name(Some(&config.empty_icon));
        status_page.set_visible(true);

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(card_list.widget()));
        overlay.add_overlay(&status_page);

        let scrolled_window = gtk::ScrolledWindow::new();
        scrolled_window.set_hexpand(true);
        scrolled_window.set_vexpand(true);
        scrolled_window.set_min_content_width(250);
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

        card_list.bind(&model, worker);

        Self {
            widget,
            status_page,
            scrolled_window,
            card_list,
            model,
            update_event: Box::new(update_event),
        }
    }

}

impl<M: CardListModel + 'static> EventListener for CardListPage<M> {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::Started => {}
            AppEvent::LoginEvent(LoginEvent::LoginCompleted) => {
                self.model.refresh();
            }
            _ => {}
        }
        if (self.update_event)(event) {
            self.status_page.set_visible(!self.model.has_items());
            // If content doesn't fill the viewport, load more
            let adj = self.scrolled_window.vadjustment();
            if adj.upper() <= adj.page_size() {
                self.model.load_more();
            }
        }
    }
}

impl<M: CardListModel + 'static> Component for CardListPage<M> {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.widget.as_ref()
    }
}
