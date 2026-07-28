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
    split_view: libadwaita::NavigationSplitView,
    navigation_stack: gtk::Stack,
    home_listbox: gtk::ListBox,
    screen_factory: ScreenFactory,
    children: Vec<Box<dyn ListenerComponent>>,
}

impl Navigation {
    pub fn new(
        model: NavigationModel,
        split_view: libadwaita::NavigationSplitView,
        navigation_stack: gtk::Stack,
        home_listbox: gtk::ListBox,
        screen_factory: ScreenFactory,
        window: libadwaita::ApplicationWindow,
    ) -> Self {
        let model = Rc::new(model);

        // "win.show-sidebar" toggle action. NavigationSplitView has no such
        // property, so `collapsed` handles mobile while the desktop show/hide
        // detaches the sidebar page and zeroes its width to fill the content.
        let sidebar_page = split_view.sidebar();
        // Captured so hide/show can restore the split view's widths exactly.
        let min_sidebar_width = split_view.min_sidebar_width();
        let max_sidebar_width = split_view.max_sidebar_width();
        // Set when the user hid the sidebar on desktop, to restore on return.
        let desktop_hidden = Rc::new(Cell::new(false));

        let show_sidebar_action = gio::SimpleAction::new_stateful(
            "show-sidebar",
            None,
            &sidebar_is_visible(&split_view).to_variant(),
        );
        show_sidebar_action.connect_change_state(clone!(
            #[weak]
            split_view,
            #[strong]
            sidebar_page,
            #[strong]
            desktop_hidden,
            move |action, state| {
                let want_visible = state.and_then(|s| s.get::<bool>()).unwrap_or(true);
                if split_view.is_collapsed() {
                    // Mobile: switch between the sidebar pane and the page.
                    split_view.set_show_content(!want_visible);
                } else {
                    // Desktop: hide by detaching the sidebar, show by re-attaching.
                    desktop_hidden.set(!want_visible);
                    set_desktop_sidebar(
                        &split_view,
                        &sidebar_page,
                        !want_visible,
                        min_sidebar_width,
                        max_sidebar_width,
                    );
                }
                action.set_state(&want_visible.to_variant());
            }
        ));
        window.add_action(&show_sidebar_action);

        // On mobile/desktop switch: toggle styling, keep the sidebar reachable
        // on mobile or re-apply the desktop preference, and sync state.
        split_view.connect_collapsed_notify(clone!(
            #[weak]
            model,
            #[weak]
            show_sidebar_action,
            #[strong]
            sidebar_page,
            #[strong]
            desktop_hidden,
            move |split_view| {
                let folded = split_view.is_collapsed();
                if folded {
                    // Mobile needs the sidebar attached so it can be reached.
                    set_desktop_sidebar(
                        split_view,
                        &sidebar_page,
                        false,
                        min_sidebar_width,
                        max_sidebar_width,
                    );
                    split_view.add_css_class("collapsed");
                    split_view.set_show_content(true);
                } else {
                    set_desktop_sidebar(
                        split_view,
                        &sidebar_page,
                        desktop_hidden.get(),
                        min_sidebar_width,
                        max_sidebar_width,
                    );
                    split_view.remove_css_class("collapsed");
                }
                let is_main = split_view.shows_content();
                model.set_nav_hidden(folded && is_main);
                sync_show_sidebar_state(&show_sidebar_action, split_view);
            }
        ));

        // On visible-pane change: keep styling, nav-hidden and toggle in sync.
        split_view.connect_show_content_notify(clone!(
            #[weak]
            model,
            #[weak]
            show_sidebar_action,
            move |split_view| {
                let folded = split_view.is_collapsed();
                if folded {
                    split_view.add_css_class("collapsed");
                } else {
                    split_view.remove_css_class("collapsed");
                }
                let is_main = split_view.shows_content();
                model.set_nav_hidden(folded && is_main);
                sync_show_sidebar_state(&show_sidebar_action, split_view);
            }
        ));

        Self {
            model,
            split_view,
            navigation_stack,
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

    fn show_navigation(&self) {
        self.split_view.set_show_content(false);
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

        self.split_view.set_show_content(true);
        self.navigation_stack
            .add_named(&widget, Some(name.identifier().as_ref()));
        self.navigation_stack
            .set_visible_child_name(name.identifier().as_ref());

        glib::source::idle_add_local_once(move || {
            widget.grab_focus();
        });
    }

    fn pop(&mut self) {
        let children = &mut self.children;
        let popped = children.pop();

        let name = self.model.visible_child_name();
        self.navigation_stack
            .set_visible_child_name(name.identifier().as_ref());

        if let Some(child) = popped {
            self.navigation_stack.remove(child.get_root_widget());
        }
    }

    fn pop_to(&mut self, screen: &ScreenName) {
        self.navigation_stack
            .set_visible_child_name(screen.identifier().as_ref());
        let remainder = self.children.split_off(self.model.children_count());
        for widget in remainder {
            self.navigation_stack.remove(widget.get_root_widget());
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
                self.split_view.set_show_content(true);
            }
            _ => {}
        };
        for child in self.children.iter_mut() {
            child.on_event(event);
        }
    }
}

/// Whether the sidebar is on screen: shown pane on mobile, attached page on
/// desktop.
fn sidebar_is_visible(split_view: &libadwaita::NavigationSplitView) -> bool {
    if split_view.is_collapsed() {
        !split_view.shows_content()
    } else {
        split_view.sidebar().is_some()
    }
}

/// Show or hide the sidebar on the desktop layout by detaching the page and
/// zeroing its width (showing restores it). Avoids `collapsed`/`show-content`
/// so the mobile flow is untouched.
fn set_desktop_sidebar(
    split_view: &libadwaita::NavigationSplitView,
    sidebar_page: &Option<libadwaita::NavigationPage>,
    hidden: bool,
    min_width: f64,
    max_width: f64,
) {
    if hidden {
        split_view.set_sidebar(libadwaita::NavigationPage::NONE);
        split_view.set_min_sidebar_width(0.0);
        split_view.set_max_sidebar_width(0.0);
    } else {
        if split_view.sidebar().is_none() {
            split_view.set_sidebar(sidebar_page.as_ref());
        }
        split_view.set_min_sidebar_width(min_width);
        split_view.set_max_sidebar_width(max_width);
    }
}

/// Sync the "show-sidebar" action state with actual sidebar visibility.
fn sync_show_sidebar_state(
    action: &gio::SimpleAction,
    split_view: &libadwaita::NavigationSplitView,
) {
    let visible = sidebar_is_visible(split_view);
    if action.state().and_then(|s| s.get::<bool>()) != Some(visible) {
        action.set_state(&visible.to_variant());
    }
}
