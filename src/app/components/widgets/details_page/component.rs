use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use super::{is_playback_event, DetailsPage, PageModel};
use crate::app::components::{
    CardLayout, CardList, CardListModel, CardSize, Component, EmbeddedCardList, EventListener,
    FilterToggle, HeaderBarModel, HeaderRegistrar, Playlist, PlaylistModel, SortOrder,
};
use crate::app::dispatch::Worker;
use crate::app::{ActionDispatcher, AppEvent};

/// A generic details page component that wires all standard behavior
/// from a `PageModel` implementation automatically.
pub struct DetailsPageComponent<M> {
    model: Rc<M>,
    worker: Worker,
    page: DetailsPage,
    content: gtk::Box,
    children: Vec<Box<dyn EventListener>>,
    registrar: HeaderRegistrar,
    name: String,
    header_title: libadwaita::WindowTitle,
    end_box: gtk::Box,
}

impl<M: PageModel + 'static> DetailsPageComponent<M> {
    /// Create a details page with an internal content box.
    ///
    /// Use [`Self::create_playlist`] and [`Self::create_card_list`] to append
    /// widgets into the content area in call order.
    pub fn new<H: HeaderBarModel + 'static>(
        model: Rc<M>,
        headerbar_model: Rc<H>,
        worker: Worker,
        registrar: HeaderRegistrar,
        name: String,
    ) -> Self {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let page = DetailsPage::new(model.header_image_shape(), &content);

        // Register this screen's header contribution: a scroll-revealed title,
        // an end-button container, and the selection/back model.
        let header_title = registrar.add_title_widget(&name);
        page.connect_title_reveal(&header_title);
        let end_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        registrar.add_end(&name, &end_box);
        registrar.register_model(&name, headerbar_model);

        let mut c = Self {
            model,
            worker,
            page,
            content,
            children: vec![],
            registrar,
            name,
            header_title,
            end_box,
        };
        c.wire();
        c
    }

    /// Append a widget to this page's end area in the shared header.
    pub fn add_header_end(&self, widget: &impl IsA<gtk::Widget>) {
        self.end_box.append(widget);
    }

    /// Create a [`Playlist`] child, appending an optional label and a `ListView`
    /// to the content box. Registers the playlist as an event listener.
    pub fn create_playlist(&mut self, label: Option<&str>)
    where
        M: PlaylistModel,
    {
        if let Some(text) = label {
            let lbl = gtk::Label::builder()
                .label(text)
                .halign(gtk::Align::Start)
                .css_classes(["title-4", "skeleton"])
                .margin_bottom(16)
                .build();
            self.content.append(&lbl);
        }
        let listview = gtk::ListView::new(None::<gtk::NoSelection>, None::<gtk::ListItemFactory>);
        listview.set_margin_bottom(16);
        self.content.append(&listview);
        let playlist = Box::new(Playlist::new(
            listview,
            self.model.clone(),
            self.worker.clone(),
        ));
        self.children.push(playlist);
    }

    /// Create an [`EmbeddedCardList`] with view controls, appending it to the content box
    /// and registering it as an event listener. Packs the view button into the headerbar.
    /// If the model provides filter options, a filter toggle bar is shown inline with the label.
    pub fn create_embedded_card_list(
        &mut self,
        label: Option<&str>,
        page_id: &str,
        available_sorts: &[SortOrder],
        shared_layout: Rc<Cell<CardLayout>>,
        shared_size: Rc<Cell<CardSize>>,
        dispatcher: Rc<dyn ActionDispatcher>,
    ) where
        M: CardListModel,
    {
        let filter_options = self.model.filter_options();
        let has_filters = !filter_options.is_empty();

        let card_list = Rc::new(CardList::new());
        card_list.widget().set_margin_bottom(16);

        if has_filters {
            // Header row: label (left, hexpand) + filter toggle (right, shrinkable)
            let header_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            header_row.set_margin_bottom(10);

            if let Some(text) = label {
                let lbl = gtk::Label::builder()
                    .label(text)
                    .halign(gtk::Align::Start)
                    .hexpand(true)
                    .css_classes(["title-4", "skeleton"])
                    .build();
                header_row.append(&lbl);
            }

            // Empty state label shown when a filter matches nothing
            let empty_label = gtk::Label::builder()
                .label("")
                .halign(gtk::Align::Center)
                .valign(gtk::Align::Center)
                .margin_top(24)
                .margin_bottom(24)
                .css_classes(["dim-label"])
                .visible(false)
                .build();

            let empty_label_ref = empty_label.clone();
            let filter_widget = FilterToggle::new(
                &filter_options,
                Rc::clone(&card_list),
                move |category, visible_count| {
                    if category.is_empty() {
                        empty_label_ref.set_visible(false);
                    } else if visible_count == 0 {
                        let msg = gettextrs::gettext("No items found for this filter");
                        empty_label_ref.set_label(&msg);
                        empty_label_ref.set_visible(true);
                    } else {
                        empty_label_ref.set_visible(false);
                    }
                },
            );

            header_row.append(&filter_widget);
            self.content.append(&header_row);
            self.content.append(&empty_label);
        } else if let Some(text) = label {
            // No filters - just append a plain label
            let lbl = gtk::Label::builder()
                .label(text)
                .halign(gtk::Align::Start)
                .hexpand(true)
                .css_classes(["title-4", "skeleton"])
                .margin_bottom(10)
                .build();
            self.content.append(&lbl);
        }

        self.content.append(card_list.widget());

        card_list.bind(
            &self.model,
            self.worker.clone(),
            CardLayout::Vertical,
            CardSize::Large,
        );
        card_list.show_placeholders();

        let embedded = EmbeddedCardList::new(
            card_list,
            page_id,
            available_sorts,
            shared_layout,
            shared_size,
            dispatcher,
        );
        self.add_header_end(embedded.view_button());
        self.children.push(Box::new(embedded));
    }

    pub fn page(&self) -> &DetailsPage {
        &self.page
    }

    pub fn model(&self) -> &Rc<M> {
        &self.model
    }

    pub fn add_child(&mut self, child: Box<dyn EventListener>) {
        self.children.push(child);
    }

    /// Wire up signal handlers and initial state based on the model's `PageModel` impl.
    /// Called once during construction.
    fn wire(&mut self) {
        if self.model.has_play_button() {
            self.page.header().connect_play(clone!(
                #[weak(rename_to = m)]
                self.model,
                move || m.toggle_play()
            ));
            self.page.header().connect_shuffle(clone!(
                #[weak(rename_to = m)]
                self.model,
                move || m.shuffle_play()
            ));
        }

        if self.model.has_like_button() {
            self.page.header().connect_liked(clone!(
                #[weak(rename_to = m)]
                self.model,
                move || m.toggle_like()
            ));
        }

        if self.model.has_info_button() {
            self.page.header().connect_info(clone!(
                #[weak(rename_to = m)]
                self.model,
                move || m.on_info_clicked()
            ));
        }

        if self.model.has_share_button() {
            self.page.header().connect_share(clone!(
                #[weak(rename_to = m)]
                self.model,
                move || m.on_share_clicked()
            ));
        }

        self.page.connect_bottom_edge(clone!(
            #[weak(rename_to = m)]
            self.model,
            move || m.load_more()
        ));

        // Initial state
        if let Some(icon) = self.model.default_icon() {
            self.page.header().set_default_icon(icon);
        }
        if self.model.is_loaded() {
            self.refresh_details();
        } else {
            self.model.load_page_info();
        }
    }

    /// Refresh the page header from the model's current state.
    pub fn refresh_details(&self) {
        if let Some(title) = self.model.get_title() {
            let subtitle = self.model.get_subtitle().unwrap_or_default();
            self.page.set_details(&title, &subtitle);
            self.header_title.set_title(&title);
            self.header_title.set_subtitle(&subtitle);
        }

        // Set subtitle links if the model provides them
        let links = self.model.get_subtitle_links();
        if !links.is_empty() {
            let artists: Vec<(String, String)> = links
                .iter()
                .map(|a| (a.id.clone(), a.name.clone()))
                .collect();
            self.page.header().set_subtitle_links(
                &artists,
                clone!(
                    #[weak(rename_to = m)]
                    self.model,
                    move |id| {
                        m.navigate_to_subtitle_link(id);
                    }
                ),
            );
        }

        if let Some(caption) = self.model.get_caption() {
            self.page.header().set_caption(&caption);
            self.page.header().set_caption_visible(true);
        }
        if self.model.has_like_button() {
            self.page.header().set_liked(self.model.is_liked());
            if !self.model.like_visible() {
                self.page.header().set_like_visible(false);
            }
        }
        self.page
            .load_artwork_or_finish(self.model.get_artwork().as_ref(), &self.worker);
    }

    /// Standard event handling. Returns true if the event was consumed.
    pub fn handle_event(&self, event: &AppEvent) -> bool {
        match event {
            AppEvent::BrowserEvent(crate::app::BrowserEvent::SongPlaybackRequested(id))
                if self.model.has_play_button() =>
            {
                self.model.start_play(id);
                return true;
            }
            _ => (),
        }

        if self.model.should_refresh_details(event) {
            self.refresh_details();
            if self.model.has_play_button() {
                self.page
                    .header()
                    .set_playing(self.model.source_is_playing());
            }
            return true;
        }
        if self.model.should_refresh_liked(event) {
            if self.model.has_like_button() {
                self.page.header().set_liked(self.model.is_liked());
                if !self.model.like_visible() {
                    self.page.header().set_like_visible(false);
                }
            }
            return true;
        }
        if let Some(playing) = is_playback_event(event) {
            if self.model.has_play_button() {
                self.page
                    .header()
                    .set_playing(self.model.source_is_playing() && playing);
            }
            return true;
        }
        false
    }
}

impl<M: PageModel + 'static> Component for DetailsPageComponent<M> {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.page.widget().upcast_ref()
    }
    fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
        Some(&mut self.children)
    }
}

impl<M> Drop for DetailsPageComponent<M> {
    fn drop(&mut self) {
        // Unregister from the shared header so a re-push re-registers cleanly.
        self.registrar.remove(&self.name);
    }
}

impl<M: PageModel + 'static> EventListener for DetailsPageComponent<M> {
    fn on_event(&mut self, event: &AppEvent) {
        self.handle_event(event);
        self.broadcast_event(event);
    }
}

/// Generates the `Component` impl for a page struct that wraps `DetailsPageComponent`.
/// Expects the struct to have a field named `component`.
#[macro_export]
macro_rules! impl_details_component {
    ($ty:ty) => {
        impl Component for $ty {
            fn get_root_widget(&self) -> &gtk::Widget {
                self.component.get_root_widget()
            }
            fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
                self.component.get_children()
            }
        }

        impl EventListener for $ty {
            fn on_event(&mut self, event: &AppEvent) {
                self.component.handle_event(event);
                self.broadcast_event(event);
            }
        }
    };
}
