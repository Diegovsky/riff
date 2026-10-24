use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gio::prelude::{ActionExt, ActionMapExt};
use glib::prelude::ToVariant;
use glib::SignalHandlerId;

use crate::app::components::{EventListener, ListenerComponent};
use crate::app::state::{Direction, Route, Transition};
use crate::app::{AppEvent, BrowserEvent};

use super::page_stack::{hide_pages_not_showing, show_page};
use super::{factory::ScreenFactory, home::HomePane, NavigationModel};

/// Horizontal touchpad travel, in pixels, that counts as a back/forward swipe.
const TOUCHPAD_SWIPE_DISTANCE: f64 = 200.0;
/// Horizontal touch velocity, in pixels per second, that counts as a swipe.
const TOUCH_SWIPE_VELOCITY: f64 = 1000.0;

pub struct Navigation {
    model: Rc<NavigationModel>,
    split_view: libadwaita::OverlaySplitView,
    content_stack: gtk::Stack,
    home_listbox: gtk::ListBox,
    screen_factory: ScreenFactory,
    screens: Vec<(String, Box<dyn ListenerComponent>)>,
    pending_restore: Option<PendingRestore>,
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

        connect_back_forward_input(&model, &window, &content_stack);
        hide_pages_not_showing(&content_stack);

        Self {
            model,
            split_view,
            content_stack,
            home_listbox,
            screen_factory,
            screens: vec![],
            pending_restore: None,
        }
    }

    fn make_screen(&self, route: &Route) -> Box<dyn ListenerComponent> {
        match route {
            Route::Home(_) => Box::new(HomePane::new(
                self.home_listbox.clone(),
                &self.screen_factory,
            )),
            Route::Album(id) => Box::new(self.screen_factory.make_album_details(id.to_owned())),
            Route::Search => Box::new(self.screen_factory.make_search_results()),
            Route::Artist(id) => Box::new(self.screen_factory.make_artist_details(id.to_owned())),
            Route::Playlist(id) => {
                Box::new(self.screen_factory.make_playlist_details(id.to_owned()))
            }
            Route::User(id) => Box::new(self.screen_factory.make_user_details(id.to_owned())),
        }
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

    fn ensure_screen(&mut self, route: &Route) -> bool {
        let key = route.screen_key();
        if self.screens.iter().any(|(k, _)| *k == key) {
            return false;
        }
        let component = self.make_screen(route);
        self.content_stack
            .add_named(component.get_root_widget(), Some(&key));
        self.screens.push((key, component));
        true
    }

    fn show_screen(&mut self, route: &Route) {
        self.ensure_screen(route);
        show_page(&self.content_stack, &route.screen_key());
    }

    fn save_scroll(&self, transition: &Transition) {
        let scroller = self
            .content_stack
            .visible_child()
            .and_then(|screen| visible_scroller(&screen));
        if let Some(scroller) = scroller {
            self.model
                .save_scroll(transition.from_entry, scroller.vadjustment().value());
        }
    }

    fn go(&mut self, transition: &Transition) {
        self.save_scroll(transition);
        self.cancel_restore();

        let transition_type = match transition.direction {
            _ if transition.from.same_screen(&transition.to) => gtk::StackTransitionType::None,
            Direction::Forward => gtk::StackTransitionType::SlideLeft,
            Direction::Backward => gtk::StackTransitionType::SlideRight,
        };
        self.content_stack.set_transition_type(transition_type);
        self.show_screen(&transition.to);
        self.show_content();

        for route in &transition.evicted {
            let key = route.screen_key();
            if let Some(i) = self.screens.iter().position(|(k, _)| *k == key) {
                let (_, component) = self.screens.remove(i);
                self.content_stack.remove(component.get_root_widget());
            }
        }

        if let Some(widget) = self.content_stack.visible_child() {
            glib::source::idle_add_local_once(move || {
                widget.grab_focus();
            });
        }
    }

    fn restore_scroll(&mut self, scroll: u32) {
        let scroller = self
            .content_stack
            .visible_child()
            .and_then(|screen| visible_scroller(&screen));
        if let Some(scroller) = scroller {
            self.pending_restore = PendingRestore::start(scroller.vadjustment(), f64::from(scroll));
        }
    }

    fn cancel_restore(&mut self) {
        if let Some(pending) = self.pending_restore.take() {
            pending.cancel();
        }
    }
}

impl EventListener for Navigation {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::Started => {
                self.show_screen(&Route::Home(Default::default()));
            }
            AppEvent::BrowserEvent(BrowserEvent::NavigationChanged(transition)) => {
                self.go(transition);
            }
            AppEvent::BrowserEvent(BrowserEvent::NavigationHidden(false)) => {
                self.show_navigation();
            }
            _ => {}
        };
        for (_, screen) in self.screens.iter_mut() {
            screen.on_event(event);
        }
        // After the screens handled it, so the home pane shows the right page
        if let AppEvent::BrowserEvent(BrowserEvent::NavigationChanged(Transition {
            restore_scroll: Some(scroll),
            ..
        })) = event
        {
            self.restore_scroll(*scroll);
        }
    }
}

struct PendingRestore {
    adjustment: gtk::Adjustment,
    handler: Rc<RefCell<Option<SignalHandlerId>>>,
}

impl PendingRestore {
    fn start(adjustment: gtk::Adjustment, target: f64) -> Option<Self> {
        let max = |adj: &gtk::Adjustment| adj.upper() - adj.page_size();
        adjustment.set_value(target.min(max(&adjustment)));
        if max(&adjustment) >= target {
            return None;
        }

        let handler: Rc<RefCell<Option<SignalHandlerId>>> = Default::default();
        let id = adjustment.connect_upper_notify(clone!(
            #[strong]
            handler,
            move |adj| {
                adj.set_value(target.min(max(adj)));
                if max(adj) >= target {
                    if let Some(id) = handler.borrow_mut().take() {
                        adj.disconnect(id);
                    }
                }
            }
        ));
        handler.replace(Some(id));
        Some(Self {
            adjustment,
            handler,
        })
    }

    fn cancel(self) {
        if let Some(id) = self.handler.borrow_mut().take() {
            self.adjustment.disconnect(id);
        }
    }
}

fn visible_scroller(widget: &gtk::Widget) -> Option<gtk::ScrolledWindow> {
    if let Some(scroller) = widget.downcast_ref::<gtk::ScrolledWindow>() {
        return Some(scroller.clone());
    }
    if let Some(stack) = widget.downcast_ref::<gtk::Stack>() {
        return stack.visible_child().and_then(|c| visible_scroller(&c));
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        if c.is_visible() {
            if let Some(scroller) = visible_scroller(&c) {
                return Some(scroller);
            }
        }
        child = c.next_sibling();
    }
    None
}

fn connect_back_forward_input(
    model: &Rc<NavigationModel>,
    window: &libadwaita::ApplicationWindow,
    content_stack: &gtk::Stack,
) {
    const MOUSE_BACK: u32 = 8;
    const MOUSE_FORWARD: u32 = 9;

    let click = gtk::GestureClick::new();
    click.set_button(0);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    click.connect_pressed(clone!(
        #[weak]
        model,
        move |gesture, _, _, _| {
            match gesture.current_button() {
                MOUSE_BACK => model.go_back(),
                MOUSE_FORWARD => model.go_forward(),
                _ => return,
            }
            gesture.set_state(gtk::EventSequenceState::Claimed);
        }
    ));
    window.add_controller(click);

    let swipe = gtk::GestureSwipe::new();
    swipe.set_touch_only(true);
    swipe.connect_swipe(clone!(
        #[weak]
        model,
        move |_, vx, vy| {
            if vx.abs() < TOUCH_SWIPE_VELOCITY || vx.abs() < 2.0 * vy.abs() {
                return;
            }
            // Content follows the finger: dragging it right reveals what came before
            if vx > 0.0 {
                model.go_back();
            } else {
                model.go_forward();
            }
        }
    ));
    content_stack.add_controller(swipe);

    // Touchpad two-finger swipes arrive as horizontal scrolling in pixels
    let travelled = Rc::new(Cell::new(0.0));
    let fired = Rc::new(Cell::new(false));
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::HORIZONTAL);
    scroll.connect_scroll_begin(clone!(
        #[strong]
        travelled,
        #[strong]
        fired,
        move |_| {
            travelled.set(0.0);
            fired.set(false);
        }
    ));
    scroll.connect_scroll(clone!(
        #[weak]
        model,
        #[strong]
        travelled,
        #[strong]
        fired,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |controller, dx, _| {
            if controller.unit() != gdk::ScrollUnit::Surface || fired.get() {
                return glib::Propagation::Proceed;
            }
            travelled.set(travelled.get() + dx);
            if travelled.get().abs() < TOUCHPAD_SWIPE_DISTANCE {
                return glib::Propagation::Proceed;
            }
            fired.set(true);
            if travelled.get() < 0.0 {
                model.go_back();
            } else {
                model.go_forward();
            }
            glib::Propagation::Stop
        }
    ));
    content_stack.add_controller(scroll);
}

/// Collapsed with the navigation panel hidden, going back shows it.
fn sync_navigation_hidden(model: &NavigationModel, split_view: &libadwaita::OverlaySplitView) {
    model.set_nav_hidden(split_view.is_collapsed() && !split_view.shows_sidebar());
}
