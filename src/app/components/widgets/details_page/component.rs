use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::header::DETAIL_SEPARATOR;
use super::{is_playback_event, DetailsHeader, DetailsPage, DetailsSectionWidget, PinnedPageModel};
use crate::app::components::{
    filter_matches_nothing, labels, CardLayout, CardList, CardListModel, CardSize, Component,
    EmbeddedCardList, EventListener, HeaderBarModel, PageHeader, SortOrder, TrackList,
    TrackListModel,
};
use crate::app::{AppEvent, Dispatcher};
use riff_config::feature_flags::{is_enabled, FeatureFlag};

const QUEUE: &str = "queue";

/// A generic details page component that wires all standard behavior
/// from a `PageModel` implementation automatically.
pub struct DetailsPageComponent<M> {
    model: Rc<M>,
    page: DetailsPage,
    content: gtk::Box,
    children: Vec<Box<dyn EventListener>>,
    header: PageHeader,
    // Header menu entries added by the page
    page_menu: Rc<RefCell<Vec<(String, String, Rc<dyn Fn()>)>>>,
}

/// Sync the pin segment of the like+pin control from the model's current state.
fn set_pin_button_state<M: PinnedPageModel>(model: &M, header: &DetailsHeader) {
    let pin_enabled = is_enabled(FeatureFlag::PinnedObjects);
    header.set_pin_enabled(pin_enabled);
    let pin_visible = pin_enabled && model.is_liked();
    header.set_pin_visible(pin_visible);
    if pin_visible {
        header.set_pinned(model.is_pinned());
    }
}

impl<M: PinnedPageModel + 'static> DetailsPageComponent<M> {
    /// Create a details page with an internal content box.
    ///
    /// Use [`Self::create_track_list`] and [`Self::create_card_list`] to append
    /// widgets into the content area in call order.
    pub fn new<H: HeaderBarModel + 'static>(
        model: Rc<M>,
        headerbar_model: Rc<H>,
        header: PageHeader,
    ) -> Self {
        let page = DetailsPage::new(model.header_image_shape());
        let content = page.content().clone();

        header.set_title_revealed(false);
        page.connect_title_reveal(clone!(
            #[strong]
            header,
            move |revealed| header.set_title_revealed(revealed)
        ));
        header.set_model(headerbar_model);

        let mut c = Self {
            model,
            page,
            content,
            children: vec![],
            header,
            page_menu: Default::default(),
        };
        c.wire();
        c
    }

    pub fn header(&self) -> &PageHeader {
        &self.header
    }

    /// For content with its own component, like a `QueueList`.
    pub fn append_content(&self, widget: &impl IsA<gtk::Widget>) {
        self.content.append(widget);
    }

    /// Create a [`TrackList`] child, appending an optional label and a `ListView`
    /// to the content box. Registers the track list as an event listener.
    pub fn create_track_list(&mut self, label: Option<&str>) -> gtk::ListView
    where
        M: TrackListModel,
    {
        let listview = gtk::ListView::new(None::<gtk::NoSelection>, None::<gtk::ListItemFactory>);
        let section = DetailsSectionWidget::new(label, &listview);
        section.add_css_class("details-section--tracks");
        self.content.append(&section);

        let track_list = TrackList::new(listview.clone(), self.model.clone());
        // Requires an ancestor ScrolledWindow, provided by DetailsPage; must
        // run after the listview above is appended into the page's content.
        track_list.connect_scrolling();
        self.children.push(Box::new(track_list));
        listview
    }

    /// Create an [`EmbeddedCardList`] with view controls, appending it to the content box
    /// and registering it as an event listener.
    ///
    /// The view button sits inline beside the label, and moves into the headerbar
    /// once the label row scrolls out of view. Returns the card grid.
    pub fn create_embedded_card_list(
        &mut self,
        label: Option<&str>,
        page_id: &str,
        available_sorts: &[SortOrder],
        shared_layout: Rc<Cell<CardLayout>>,
        shared_size: Rc<Cell<CardSize>>,
        dispatcher: Dispatcher,
    ) -> gtk::Widget
    where
        M: CardListModel,
    {
        let card_list = Rc::new(CardList::new());
        let grid = card_list.widget().clone();
        let section = DetailsSectionWidget::new(label, card_list.widget());
        self.content.append(&section);

        let on_filter_changed = clone!(
            #[weak]
            section,
            move |category: &str, visible_count: usize| {
                section.set_empty(filter_matches_nothing(category, visible_count));
            }
        );

        card_list.bind(&self.model, CardLayout::Vertical, CardSize::Large);
        card_list.show_placeholders();

        let embedded = EmbeddedCardList::new(
            card_list,
            page_id,
            available_sorts,
            shared_layout,
            shared_size,
            &self.model.filter_options(),
            on_filter_changed,
            {
                let model = Rc::downgrade(&self.model);
                move || model.upgrade().and_then(|m| m.playing_card())
            },
            dispatcher,
        );

        let button = embedded.view_button().clone();
        section.set_button(&button);
        let button_slot = section.button_slot().clone();

        // Weak, so the handler on the page's scroll adjustment doesn't keep
        // the button and its slots alive.
        self.page.connect_scrolled_past(
            section.header_row().upcast_ref(),
            clone!(
                #[weak]
                button,
                #[weak]
                button_slot,
                #[strong(rename_to = header)]
                self.header,
                move |past| {
                    if past {
                        // Keep the row's height once the button leaves it, so
                        // the content below doesn't shift.
                        button_slot.set_height_request(button_slot.height());
                        if button.parent().as_ref() == Some(button_slot.upcast_ref()) {
                            button_slot.remove(&button);
                        }
                        header.add_end(&button);
                    } else {
                        header.remove_end(&button);
                        if button.parent().is_none() {
                            button_slot.append(&button);
                        }
                    }
                }
            ),
        );

        self.children.push(Box::new(embedded));
        grid
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

    /// For entries that need the page's widgets, like a dialog.
    pub fn add_menu_entry(&self, id: &str, label: &str, on_activate: impl Fn() + 'static) {
        self.page_menu
            .borrow_mut()
            .push((id.to_string(), label.to_string(), Rc::new(on_activate)));
        self.refresh_menu();
    }

    fn refresh_menu(&self) {
        let mut queue = vec![];
        if self.model.has_queue_menu() {
            queue.push((QUEUE.to_string(), labels::ADD_TO_QUEUE.clone()));
        }
        let mut other = self.model.header_menu_entries();
        other.extend(
            self.page_menu
                .borrow()
                .iter()
                .map(|(id, label, _)| (id.clone(), label.clone())),
        );
        let page_menu = Rc::downgrade(&self.page_menu);
        self.page.header().set_menu(
            &[queue, other],
            clone!(
                #[weak(rename_to = m)]
                self.model,
                move |id| {
                    let page_entry = page_menu.upgrade().and_then(|entries| {
                        entries
                            .borrow()
                            .iter()
                            .find(|(entry, _, _)| entry == id)
                            .map(|(_, _, f)| f.clone())
                    });
                    match (id, page_entry) {
                        (_, Some(on_activate)) => on_activate(),
                        (QUEUE, None) => m.queue_all(),
                        (other, None) => m.on_header_menu(other),
                    }
                }
            ),
        );
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

        self.refresh_menu();

        if self.model.has_like_button() {
            self.page.header().connect_liked(clone!(
                #[weak(rename_to = m)]
                self.model,
                move || m.toggle_like()
            ));
            if self.model.supports_pin_button() {
                let (like, pin) = self.page.header().add_like_pin_segmented_button();
                like.connect_clicked(clone!(
                    #[weak(rename_to = m)]
                    self.model,
                    move |_| {
                        if m.is_liked() && m.is_pinned() {
                            m.toggle_pin();
                        }
                        m.toggle_like()
                    }
                ));
                pin.connect_clicked(clone!(
                    #[weak(rename_to = m)]
                    self.model,
                    move |_| m.toggle_pin()
                ));
            }
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

        if self.model.supports_pin_button() {
            set_pin_button_state(&*self.model, self.page.header());
        }

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
        let detail = self.model.get_subtitle_detail();
        if let Some(title) = self.model.get_title() {
            let subtitle = self.model.get_subtitle().unwrap_or_default();
            let full_subtitle = match &detail {
                Some(detail) if !subtitle.is_empty() => {
                    format!("{subtitle}{DETAIL_SEPARATOR}{detail}")
                }
                Some(detail) => detail.clone(),
                None => subtitle.clone(),
            };
            self.page.set_details(&title, &full_subtitle);
            self.header.set_title(&title);
            self.header.set_subtitle(&subtitle);
        }

        // Set subtitle links if the model provides them
        let links = self.model.get_subtitle_links();
        if !links.is_empty() {
            let artists: Vec<(String, String)> = links
                .iter()
                .map(|a| (a.rri.id.clone(), a.name.clone()))
                .collect();
            self.page.header().set_subtitle_links(
                &artists,
                detail.as_deref(),
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
            if let Some(tooltip) = self.model.like_tooltip(self.model.is_liked()) {
                self.page.header().set_like_tooltip(&tooltip);
            }
        }
        self.page
            .load_artwork_or_finish(self.model.get_artwork().as_ref(), self.model.api_service());
        if self.model.supports_pin_button() {
            set_pin_button_state(&*self.model, self.page.header());
        }
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
                // Not when paused or stopped
                self.page
                    .header()
                    .set_playing(self.model.source_is_playing() && self.model.is_playing());
            }
            return true;
        }
        if self.model.should_refresh_liked(event) {
            if self.model.has_like_button() {
                self.page.header().set_liked(self.model.is_liked());
                if !self.model.like_visible() {
                    self.page.header().set_like_visible(false);
                }
                if let Some(tooltip) = self.model.like_tooltip(self.model.is_liked()) {
                    self.page.header().set_like_tooltip(&tooltip);
                }
                if self.model.supports_pin_button() {
                    set_pin_button_state(&*self.model, self.page.header());
                }
            }
            return true;
        }
        if matches!(
            event,
            AppEvent::BrowserEvent(crate::app::BrowserEvent::PinnedPlaylistsUpdated)
        ) {
            if self.model.supports_pin_button() {
                set_pin_button_state(&*self.model, self.page.header());
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

impl<M: PinnedPageModel + 'static> Component for DetailsPageComponent<M> {
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
        self.header.remove();
    }
}

impl<M: PinnedPageModel + 'static> EventListener for DetailsPageComponent<M> {
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
