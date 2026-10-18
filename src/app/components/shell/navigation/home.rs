use gtk::prelude::*;

use crate::app::components::navigation_panel::NavigationPanelDestination;
use crate::app::components::{Component, EventListener, ScreenFactory};
use crate::app::state::{HomePage, Route, Transition};
use crate::app::{AppEvent, BrowserEvent};

pub struct HomePane {
    stack: gtk::Stack,
    components: Vec<Box<dyn EventListener>>,
}

impl HomePane {
    pub fn new(listbox: gtk::ListBox, screen_factory: &ScreenFactory) -> Self {
        let library = screen_factory.make_library();
        let saved_playlists = screen_factory.make_saved_playlists();
        let saved_tracks = screen_factory.make_saved_tracks();
        let saved_artists = screen_factory.make_saved_artists();
        let now_playing = screen_factory.make_now_playing();
        let navigation_panel = screen_factory.make_navigation_panel(listbox);

        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);

        for (page, widget) in [
            (HomePage::Library, library.get_root_widget()),
            (HomePage::SavedTracks, saved_tracks.get_root_widget()),
            (HomePage::SavedPlaylists, saved_playlists.get_root_widget()),
            (HomePage::SavedArtists, saved_artists.get_root_widget()),
            (HomePage::NowPlaying, now_playing.get_root_widget()),
        ] {
            let title = NavigationPanelDestination::from_home_page(page).title();
            stack.add_titled(widget, Some(page.id()), &title);
        }

        Self {
            stack,
            components: vec![
                Box::new(navigation_panel),
                Box::new(library),
                Box::new(saved_playlists),
                Box::new(saved_tracks),
                Box::new(saved_artists),
                Box::new(now_playing),
            ],
        }
    }
}

impl Component for HomePane {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.stack.upcast_ref()
    }

    fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
        Some(&mut self.components)
    }
}

impl EventListener for HomePane {
    fn on_event(&mut self, event: &AppEvent) {
        if let AppEvent::BrowserEvent(BrowserEvent::NavigationChanged(Transition {
            to: Route::Home(page),
            ..
        })) = event
        {
            self.stack.set_visible_child_name(page.id());
        }
        self.broadcast_event(event);
    }
}
