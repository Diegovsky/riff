use gtk::prelude::WidgetExt;
use std::cell::Cell;
use std::rc::Rc;

use gio::prelude::{ActionExt, ActionMapExt};
use glib::prelude::ToVariant;

use crate::app::components::{EventListener, ListenerComponent};
use crate::app::state::ScreenName;
use crate::app::{AppEvent, BrowserEvent};

use super::{factory::ScreenFactory, home::HomePane, NavigationModel};

pub struct Navigation {
    model: Rc<NavigationModel>,
    split_view: libadwaita::OverlaySplitView,
    content_stack: gtk::Stack,
    home_listbox: gtk::ListBox,
    screen_factory: ScreenFactory,
    children: Vec<Box<dyn ListenerComponent>>,
}

impl Navigation {
    pub fn new(
        model: NavigationModel,
        split_view: libadwaita::OverlaySplitView,
        content_stack: gtk::Stack,
        home_listbox: gtk::ListBox,
        screen_factory: ScreenFactory,
        window: libadwaita::ApplicationWindow,
    ) -> Self {
        let model = Rc::new(model);

        let show_navigation_panel_action = gio::SimpleAction::new_stateful(
            "show-navigation-panel",
            None,
            &split_view.shows_sidebar().to_variant(),
        );
        // The user hid it beside the content panel
        let desktop_hidden = Rc::new(Cell::new(false));
        show_navigation_panel_action.connect_change_state(clone!(
            #[weak]
            split_view,
            #[strong]
            desktop_hidden,
            move |_, state| {
                let want_visible = state.and_then(|s| s.get::<bool>()).unwrap_or(true);
                if !split_view.is_collapsed() {
                    desktop_hidden.set(!want_visible);
                }
                split_view.set_show_sidebar(want_visible);
            }
        ));
        window.add_action(&show_navigation_panel_action);

        split_view.connect_collapsed_notify(clone!(
            #[weak]
            model,
            #[strong]
            desktop_hidden,
            move |split_view| {
                split_view.set_show_sidebar(!split_view.is_collapsed() && !desktop_hidden.get());
                sync_navigation_hidden(&model, split_view);
            }
        ));

        split_view.connect_show_sidebar_notify(clone!(
            #[weak]
            model,
            #[weak]
            show_navigation_panel_action,
            move |split_view| {
                let visible = split_view.shows_sidebar();
                if show_navigation_panel_action
                    .state()
                    .and_then(|s| s.get::<bool>())
                    != Some(visible)
                {
                    show_navigation_panel_action.set_state(&visible.to_variant());
                }
                sync_navigation_hidden(&model, split_view);
            }
        ));

        // Hide scrollbars on the stack's pages while the slide transition is running
        content_stack.connect_transition_running_notify(|stack| {
            if stack.is_transition_running() {
                stack.add_css_class("transitioning");
            } else {
                stack.remove_css_class("transitioning");
            }
        });

        Self {
            model,
            split_view,
            content_stack,
            home_listbox,
            screen_factory,
            children: vec![],
        }
    }

    fn make_home(&self) -> Box<dyn ListenerComponent> {
        Box::new(HomePane::new(
            self.home_listbox.clone(),
            &self.screen_factory,
        ))
    }

    // Only collapsed: showing it beside the content panel would undo a hide
    fn show_navigation(&self) {
        if self.split_view.is_collapsed() {
            self.split_view.set_show_sidebar(true);
        }
    }

    fn show_content(&self) {
        if self.split_view.is_collapsed() {
            self.split_view.set_show_sidebar(false);
        }
    }

    fn push_screen(&mut self, name: &ScreenName) {
        let component: Box<dyn ListenerComponent> = match name {
            ScreenName::Home => self.make_home(),
            ScreenName::AlbumDetails(id) => {
                Box::new(self.screen_factory.make_album_details(id.to_owned()))
            }
            ScreenName::Search => Box::new(self.screen_factory.make_search_results()),
            ScreenName::Artist(id) => {
                Box::new(self.screen_factory.make_artist_details(id.to_owned()))
            }
            ScreenName::PlaylistDetails(id) => {
                Box::new(self.screen_factory.make_playlist_details(id.to_owned()))
            }
            ScreenName::User(id) => Box::new(self.screen_factory.make_user_details(id.to_owned())),
        };

        let widget = component.get_root_widget().clone();
        self.children.push(component);

        self.show_content();
        self.content_stack
            .add_named(&widget, Some(name.identifier().as_ref()));
        self.content_stack
            .set_visible_child_name(name.identifier().as_ref());

        glib::source::idle_add_local_once(move || {
            widget.grab_focus();
        });
    }

    fn pop(&mut self) {
        let children = &mut self.children;
        let popped = children.pop();

        let name = self.model.visible_child_name();
        self.content_stack
            .set_visible_child_name(name.identifier().as_ref());

        if let Some(child) = popped {
            self.content_stack.remove(child.get_root_widget());
        }
    }

    fn pop_to(&mut self, screen: &ScreenName) {
        self.content_stack
            .set_visible_child_name(screen.identifier().as_ref());
        let remainder = self.children.split_off(self.model.children_count());
        for widget in remainder {
            self.content_stack.remove(widget.get_root_widget());
        }
    }
}

impl EventListener for Navigation {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::Started => {
                self.push_screen(&ScreenName::Home);
            }
            AppEvent::BrowserEvent(BrowserEvent::NavigationPushed(name)) => {
                self.push_screen(name);
            }
            AppEvent::BrowserEvent(BrowserEvent::NavigationHidden(false)) => {
                self.show_navigation();
            }
            AppEvent::BrowserEvent(BrowserEvent::NavigationPopped) => {
                self.pop();
            }
            AppEvent::BrowserEvent(BrowserEvent::NavigationPoppedTo(name)) => {
                self.pop_to(name);
            }
            AppEvent::BrowserEvent(BrowserEvent::HomeVisiblePageChanged(_)) => {
                self.show_content();
            }
            _ => {}
        };
        for child in self.children.iter_mut() {
            child.on_event(event);
        }
    }
}

/// Collapsed with the navigation panel hidden, going back shows it.
fn sync_navigation_hidden(model: &NavigationModel, split_view: &libadwaita::OverlaySplitView) {
    model.set_nav_hidden(split_view.is_collapsed() && !split_view.shows_sidebar());
}
