use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use std::cell::Cell;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::utils::Debouncer;
use crate::app::components::widgets::card_list::card_view_menu::CardViewMenu;
use crate::app::components::{
    display_add_css_provider, CardLayout, CardList, CardListModel, CardSize, CardWidget, Component,
    EventListener, HeaderRegistrar, ImageShape, Playlist, SortOrder, CLAMP_MAX_SIZE,
};
use crate::app::dispatch::Worker;
use crate::app::models::{CardModel, SearchType, SongDescription};
use crate::app::state::{AppEvent, BrowserEvent};
use crate::app::{ActionDispatcher, ListStore};

use super::{SearchResultsModel, SearchScopeCardsModel, SearchScopeTracksModel};

/// Stack page names for the different result presentations.
const PAGE_ALL: &str = "all";
const PAGE_CARDS: &str = "cards";
const PAGE_TRACKS: &str = "tracks";

/// Debounce (ms) between the last keystroke and firing a search.
const SEARCH_DEBOUNCE_MS: u32 = 600;

/// Rows shown per section in the combined view before "Show All".
const SECTION_MAX_ROWS: u32 = 2;
/// Bottom margin (px) below each section's card grid.
const SECTION_BOTTOM_MARGIN: i32 = 32;
/// Spacing (px) between sections in the combined view.
const SECTION_SPACING: i32 = 16;

mod imp {

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/search.ui")]
    pub struct SearchResultsWidget {
        #[template_child]
        pub search_bar: TemplateChild<libadwaita::Clamp>,

        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,

        #[template_child]
        pub filter_button: TemplateChild<gtk::MenuButton>,

        #[template_child]
        pub filter_popover: TemplateChild<gtk::Popover>,

        #[template_child]
        pub filter_artists: TemplateChild<gtk::ToggleButton>,

        #[template_child]
        pub filter_albums: TemplateChild<gtk::ToggleButton>,

        #[template_child]
        pub filter_playlists: TemplateChild<gtk::ToggleButton>,

        #[template_child]
        pub filter_songs: TemplateChild<gtk::ToggleButton>,

        #[template_child]
        pub status_page: TemplateChild<libadwaita::StatusPage>,

        #[template_child]
        pub search_results: TemplateChild<gtk::ScrolledWindow>,

        #[template_child]
        pub results_box: TemplateChild<gtk::Box>,

        /// Guard flag to prevent re-entrant filter toggle signals.
        pub filter_updating: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SearchResultsWidget {
        const NAME: &'static str = "SearchResultsWidget";
        type Type = super::SearchResultsWidget;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SearchResultsWidget {}
    impl BoxImpl for SearchResultsWidget {}

    impl WidgetImpl for SearchResultsWidget {
        fn grab_focus(&self) -> bool {
            self.search_entry.grab_focus()
        }
    }
}

glib::wrapper! {
    pub struct SearchResultsWidget(ObjectSubclass<imp::SearchResultsWidget>) @extends gtk::Widget, gtk::Box;
}

impl Default for SearchResultsWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchResultsWidget {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_search_updated<F>(&self, f: F)
    where
        F: Fn(String) + 'static,
    {
        self.imp().search_entry.connect_changed(clone!(
            #[weak(rename_to = _self)]
            self,
            move |s| {
                let query = s.text();
                let query = query.as_str();
                _self.imp().status_page.set_visible(query.is_empty());
                _self.imp().search_results.set_visible(!query.is_empty());
                if !query.is_empty() {
                    f(query.to_string());
                }
            }
        ));
    }

    /// The filter toggle buttons paired with the scope they select.
    fn filter_buttons(&self) -> [(gtk::ToggleButton, SearchType); 4] {
        let imp = self.imp();
        [
            ((*imp.filter_artists).clone(), SearchType::Artists),
            ((*imp.filter_albums).clone(), SearchType::Albums),
            ((*imp.filter_playlists).clone(), SearchType::Playlists),
            ((*imp.filter_songs).clone(), SearchType::Tracks),
        ]
    }

    /// Wire the filter popover toggle buttons. The callback receives the chosen
    /// filter (`None` when the active button is toggled off, returning to the
    /// combined "All" view).
    pub fn connect_filter_selected<F>(&self, f: F)
    where
        F: Fn(Option<SearchType>) + 'static,
    {
        let f = Rc::new(f);

        for (button, filter) in self.filter_buttons() {
            let f = Rc::clone(&f);
            button.connect_toggled(clone!(
                #[weak(rename_to = widget)]
                self,
                move |btn| {
                    if widget.imp().filter_updating.get() {
                        return;
                    }
                    widget.imp().filter_updating.set(true);
                    if btn.is_active() {
                        // Deactivate all other buttons.
                        for (other, _) in widget.filter_buttons() {
                            if &other != btn {
                                other.set_active(false);
                            }
                        }
                        widget.imp().filter_popover.popdown();
                        f(Some(filter));
                    } else {
                        // Toggled off - go back to "All" view.
                        widget.imp().filter_popover.popdown();
                        f(None);
                    }
                    widget.imp().filter_updating.set(false);
                }
            ));
        }
    }

    /// Visually indicate which filter is active on the popover and the button.
    pub fn set_active_filter(&self, filter: Option<SearchType>) {
        let imp = self.imp();
        imp.filter_updating.set(true);
        for (button, this_filter) in self.filter_buttons() {
            button.set_active(filter == Some(this_filter));
        }
        // Highlight the filter button itself when a scope is active.
        if filter.is_some() {
            imp.filter_button.add_css_class("accent");
        } else {
            imp.filter_button.remove_css_class("accent");
        }
        imp.filter_updating.set(false);
    }

    pub fn connect_edge_reached<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp()
            .search_results
            .connect_edge_reached(move |_, pos| {
                if pos == gtk::PositionType::Bottom {
                    f();
                }
            });
    }
}

/// A simple model adapter that allows CardList to display a search section.
struct SearchSectionModel {
    store: ListStore<CardModel>,
    shape: ImageShape,
    on_activated: Box<dyn Fn(String)>,
}

impl CardListModel for SearchSectionModel {
    fn get_store(&self) -> Option<impl Deref<Target = ListStore<CardModel>> + '_> {
        Some(&self.store)
    }

    fn load_more(&self) {}
    fn refresh(&self) {}

    fn has_items(&self) -> bool {
        self.store.len() > 0
    }

    fn open_item(&self, id: String) {
        (self.on_activated)(id);
    }

    fn image_shape(&self) -> ImageShape {
        self.shape
    }
}

/// Creates a section header row with a larger label and a "Show All" button.
fn make_section_header(text: &str, filter: SearchType, model: &Rc<SearchResultsModel>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);

    let label = gtk::Label::builder()
        .label(text)
        .halign(gtk::Align::Start)
        .hexpand(true)
        .css_classes(["title-2"])
        .build();
    row.append(&label);

    let show_all_btn = gtk::Button::builder()
        .label(&gettextrs::gettext("Show All"))
        .halign(gtk::Align::End)
        .valign(gtk::Align::Center)
        .has_frame(false)
        .css_classes(["flat", "accent"])
        .build();
    show_all_btn.connect_clicked(clone!(
        #[weak]
        model,
        move |_| model.set_filter(Some(filter))
    ));
    row.append(&show_all_btn);

    row
}

/// Build one section of the combined view (header + row-limited card grid),
/// append it to `container`, and return the bound card list.
fn build_section(
    container: &gtk::Box,
    title: &str,
    filter: SearchType,
    section: &Rc<SearchSectionModel>,
    model: &Rc<SearchResultsModel>,
    worker: &Worker,
    layout: CardLayout,
    size: CardSize,
) -> Rc<CardList> {
    container.append(&make_section_header(title, filter, model));
    let card_list = Rc::new(CardList::new());
    container.append(card_list.widget());
    card_list.bind(section, worker.clone(), layout, size);
    card_list.set_max_rows(Some(SECTION_MAX_ROWS));
    card_list.widget().set_margin_bottom(SECTION_BOTTOM_MARGIN);
    card_list
}

/// Replaces the contents of a ListStore. `ListStore::replace_all` takes `&mut
/// self`, so we operate on a cheap clone (the store is a shared GObject handle).
fn replace_store_contents(store: &ListStore<CardModel>, items: Vec<CardModel>) {
    store.clone().replace_all(items.into_iter());
}

pub struct SearchResults {
    widget: SearchResultsWidget,
    model: Rc<SearchResultsModel>,
    track_section: Rc<SearchSectionModel>,
    artist_section: Rc<SearchSectionModel>,
    album_section: Rc<SearchSectionModel>,
    playlist_section: Rc<SearchSectionModel>,
    track_card_list: Rc<CardList>,
    artist_card_list: Rc<CardList>,
    album_card_list: Rc<CardList>,
    playlist_card_list: Rc<CardList>,
    scope_cards_model: Rc<SearchScopeCardsModel>,
    scope_card_list: Rc<CardList>,
    content_stack: gtk::Stack,
    view_menu: CardViewMenu,
    layout: Rc<Cell<CardLayout>>,
    size: Rc<Cell<CardSize>>,
    worker: Worker,
    debouncer: Debouncer,
    children: Vec<Box<dyn EventListener>>,
    registrar: HeaderRegistrar,
}

impl SearchResults {
    pub fn new(
        model: SearchResultsModel,
        worker: Worker,
        layout: Rc<Cell<CardLayout>>,
        size: Rc<Cell<CardSize>>,
        dispatcher: Rc<dyn ActionDispatcher>,
        registrar: HeaderRegistrar,
    ) -> Self {
        display_add_css_provider(resource!("/components/search.css"));

        let model = Rc::new(model);
        let widget = SearchResultsWidget::new();

        // Move the search bar into the shared header as this screen's title.
        let search_bar = widget.imp().search_bar.get();
        widget.remove(&search_bar);
        registrar.add_title("search", &search_bar);

        // --- Combined ("all") view: Tracks / Artists / Albums sections ---
        let all_box = gtk::Box::new(gtk::Orientation::Vertical, SECTION_SPACING);

        let track_section = Rc::new(SearchSectionModel {
            store: ListStore::new(),
            shape: ImageShape::Square,
            // Tracks need the full SongDescription, so activation is wired
            // separately via connect_child_activated below rather than by id.
            on_activated: Box::new(|_| {}),
        });
        let artist_section = Rc::new(SearchSectionModel {
            store: ListStore::new(),
            shape: ImageShape::Round,
            on_activated: Box::new(clone!(
                #[weak]
                model,
                move |id| model.open_artist(id)
            )),
        });
        let album_section = Rc::new(SearchSectionModel {
            store: ListStore::new(),
            shape: ImageShape::Square,
            on_activated: Box::new(clone!(
                #[weak]
                model,
                move |id| model.open_album(id)
            )),
        });
        let playlist_section = Rc::new(SearchSectionModel {
            store: ListStore::new(),
            shape: ImageShape::Square,
            on_activated: Box::new(clone!(
                #[weak]
                model,
                move |id| model.open_playlist(id)
            )),
        });

        let track_card_list = build_section(
            &all_box,
            &gettextrs::gettext("Tracks"),
            SearchType::Tracks,
            &track_section,
            &model,
            &worker,
            layout.get(),
            size.get(),
        );

        // Override track activation to pass the full SongDescription.
        let track_section_clone = Rc::clone(&track_section);
        let model_weak = Rc::downgrade(&model);
        track_card_list
            .widget()
            .connect_child_activated(move |_, child| {
                let Some(card_widget) = child.child().and_then(|w| w.downcast::<CardWidget>().ok())
                else {
                    return;
                };
                let id = card_widget.card_id();
                if id.is_empty() {
                    return;
                }
                let Some(m) = model_weak.upgrade() else {
                    return;
                };
                if let Some(card_model) = track_section_clone.store.iter().find(|c| c.id() == id) {
                    if let Some(song) = card_model
                        .data()
                        .and_then(|d| d.downcast_ref::<SongDescription>().cloned())
                    {
                        m.open_track(song);
                    }
                }
            });

        let artist_card_list = build_section(
            &all_box,
            &gettextrs::gettext("Artists"),
            SearchType::Artists,
            &artist_section,
            &model,
            &worker,
            layout.get(),
            size.get(),
        );

        let album_card_list = build_section(
            &all_box,
            &gettextrs::gettext("Albums"),
            SearchType::Albums,
            &album_section,
            &model,
            &worker,
            layout.get(),
            size.get(),
        );

        let playlist_card_list = build_section(
            &all_box,
            &gettextrs::gettext("Playlists"),
            SearchType::Playlists,
            &playlist_section,
            &model,
            &worker,
            layout.get(),
            size.get(),
        );

        // --- Scoped card view (artists / albums / playlists) ---
        let scope_cards_model = Rc::new(SearchScopeCardsModel::new(
            model.app_model(),
            dispatcher.box_clone(),
        ));
        let scope_card_list = Rc::new(CardList::new());
        scope_card_list.bind(&scope_cards_model, worker.clone(), layout.get(), size.get());

        // --- Scoped track view (songs) ---
        let scope_tracks_model = Rc::new(SearchScopeTracksModel::new(
            model.app_model(),
            dispatcher.box_clone(),
        ));
        let track_listview =
            gtk::ListView::new(None::<gtk::NoSelection>, None::<gtk::ListItemFactory>);
        let scope_playlist = Playlist::new(
            track_listview.clone(),
            Rc::clone(&scope_tracks_model),
            worker.clone(),
        );

        // Constrain the track list width to match the Now Playing page.
        track_listview.set_hexpand(true);
        let tracks_clamp = libadwaita::Clamp::new();
        tracks_clamp.set_maximum_size(CLAMP_MAX_SIZE);
        tracks_clamp.set_tightening_threshold(CLAMP_MAX_SIZE);
        tracks_clamp.set_child(Some(&track_listview));

        // Constrain the combined view width to match the Now Playing / track views.
        all_box.set_hexpand(true);
        let all_clamp = libadwaita::Clamp::new();
        all_clamp.set_maximum_size(CLAMP_MAX_SIZE);
        all_clamp.set_tightening_threshold(CLAMP_MAX_SIZE);
        all_clamp.set_child(Some(&all_box));

        // --- Stack that swaps between the three presentations ---
        let content_stack = gtk::Stack::new();
        content_stack.set_vexpand(true);
        content_stack.add_named(&all_clamp, Some(PAGE_ALL));
        content_stack.add_named(scope_card_list.widget(), Some(PAGE_CARDS));
        content_stack.add_named(&tracks_clamp, Some(PAGE_TRACKS));
        content_stack.set_visible_child_name(PAGE_ALL);
        widget.imp().results_box.append(&content_stack);

        // Card view menu (layout/size/sort controls) in the headerbar.
        let current_sort = Rc::new(Cell::new(SortOrder::RecentlyAdded));
        let view_menu = CardViewMenu::new(
            "search".to_string(),
            &[],
            Rc::clone(&layout),
            Rc::clone(&size),
            current_sort,
            Rc::clone(&track_card_list),
            Rc::clone(&dispatcher),
        );
        registrar.add_end("search", view_menu.widget());

        widget.connect_search_updated(clone!(
            #[weak]
            model,
            move |q| model.search(q)
        ));

        widget.connect_filter_selected(clone!(
            #[weak]
            model,
            move |filter| model.set_filter(filter)
        ));

        // Infinite scroll for the scoped views.
        widget.connect_edge_reached(clone!(
            #[weak]
            model,
            #[weak]
            scope_cards_model,
            #[weak]
            scope_tracks_model,
            move || match model.get_filter() {
                Some(SearchType::Tracks) => scope_tracks_model.load_more(),
                Some(_) => scope_cards_model.load_more(),
                None => {}
            }
        ));

        widget.set_active_filter(model.get_filter());

        Self {
            widget,
            model,
            track_section,
            artist_section,
            album_section,
            playlist_section,
            track_card_list,
            artist_card_list,
            album_card_list,
            playlist_card_list,
            scope_cards_model,
            scope_card_list,
            content_stack,
            view_menu,
            layout,
            size,
            worker,
            debouncer: Debouncer::new(),
            children: vec![Box::new(scope_playlist)],
            registrar,
        }
    }

    fn update_results(&self) {
        let Some(results) = self.model.get_results() else {
            return;
        };

        let tracks: Vec<CardModel> = results
            .tracks
            .songs
            .iter()
            .map(|track| CardModel::from(track).with_data(track.clone()))
            .collect();
        replace_store_contents(&self.track_section.store, tracks);

        let artists: Vec<CardModel> = results.artists.iter().map(CardModel::from).collect();
        replace_store_contents(&self.artist_section.store, artists);

        let albums: Vec<CardModel> = results.albums.iter().map(CardModel::from).collect();
        replace_store_contents(&self.album_section.store, albums);

        let playlists: Vec<CardModel> = results.playlists.iter().map(CardModel::from).collect();
        replace_store_contents(&self.playlist_section.store, playlists);
    }

    /// Clear the combined-view sections and show skeleton placeholders.
    fn show_all_placeholders(&self) {
        replace_store_contents(&self.track_section.store, Vec::new());
        replace_store_contents(&self.artist_section.store, Vec::new());
        replace_store_contents(&self.album_section.store, Vec::new());
        replace_store_contents(&self.playlist_section.store, Vec::new());
        self.track_card_list.show_placeholders();
        self.artist_card_list.show_placeholders();
        self.album_card_list.show_placeholders();
        self.playlist_card_list.show_placeholders();
    }

    /// Kick off a (debounced) search for the current query + filter, showing
    /// the appropriate loading state.
    fn trigger_search(&self) {
        match self.model.get_filter() {
            None => self.show_all_placeholders(),
            Some(SearchType::Tracks) => {}
            Some(_) => self.scope_card_list.show_placeholders(),
        }
        self.debouncer.debounce(
            SEARCH_DEBOUNCE_MS,
            clone!(
                #[weak(rename_to = model)]
                self.model,
                move || model.fetch_results()
            ),
        );
    }

    /// Switch the visible presentation and re-run the search for the new filter.
    fn on_filter_changed(&self) {
        let filter = self.model.get_filter();
        self.widget.set_active_filter(filter);

        // Hide the card style/size button on the tracks page since it's a list.
        self.view_menu
            .widget()
            .set_visible(!matches!(filter, Some(SearchType::Tracks)));

        match filter {
            None => self.set_visible_page(PAGE_ALL),
            Some(SearchType::Tracks) => self.set_visible_page(PAGE_TRACKS),
            Some(_) => {
                // Rebind so the correct image shape (round for artists) applies.
                self.scope_card_list.bind(
                    &self.scope_cards_model,
                    self.worker.clone(),
                    self.layout.get(),
                    self.size.get(),
                );
                self.set_visible_page(PAGE_CARDS);
            }
        }

        if self.model.has_query() {
            self.trigger_search();
        }
    }

    fn set_visible_page(&self, name: &str) {
        self.content_stack.set_visible_child_name(name);
    }

    fn on_scope_updated(&self) {
        if !matches!(self.model.get_filter(), Some(SearchType::Tracks)) {
            self.scope_card_list.remove_placeholders();
        }
    }
}

impl Drop for SearchResults {
    fn drop(&mut self) {
        self.registrar.remove("search");
    }
}

impl Component for SearchResults {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.widget.as_ref()
    }

    fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
        Some(&mut self.children)
    }
}

impl EventListener for SearchResults {
    fn on_event(&mut self, app_event: &AppEvent) {
        match app_event {
            AppEvent::BrowserEvent(BrowserEvent::SearchUpdated) => {
                self.get_root_widget().grab_focus();
                self.trigger_search();
            }
            AppEvent::BrowserEvent(BrowserEvent::SearchFilterChanged) => {
                self.on_filter_changed();
            }
            AppEvent::BrowserEvent(BrowserEvent::SearchResultsUpdated) => {
                self.update_results();
            }
            AppEvent::BrowserEvent(BrowserEvent::SearchScopeUpdated(_)) => {
                self.on_scope_updated();
            }
            AppEvent::BrowserEvent(BrowserEvent::AlbumDetailsLoaded(id)) => {
                self.model.on_album_loaded(id);
            }
            AppEvent::BrowserEvent(
                BrowserEvent::CardLayoutChanged(_) | BrowserEvent::CardSizeChanged(_),
            ) => {
                let l = self.layout.get();
                let s = self.size.get();
                for cl in [
                    &self.track_card_list,
                    &self.artist_card_list,
                    &self.album_card_list,
                    &self.playlist_card_list,
                    &self.scope_card_list,
                ] {
                    cl.update_layout(l);
                    cl.update_size(s);
                }
                self.view_menu.sync(l);
            }
            _ => {}
        }
        self.broadcast_event(app_event);
    }
}
