use std::rc::Rc;

use gtk::prelude::*;

use super::component::{HeaderBarModel, HeaderModelRegistry};
use super::widget::AppHeaderBar;

/// Handle screens use to contribute to the shared [`AppHeaderBar`]: a title
/// widget, optional end buttons, and the [`HeaderBarModel`] driving
/// selection/back. Each contribution is keyed by the screen's page name.
#[derive(Clone)]
pub struct HeaderRegistrar {
    widget: AppHeaderBar,
    models: HeaderModelRegistry,
}

impl HeaderRegistrar {
    pub fn new(widget: AppHeaderBar, models: HeaderModelRegistry) -> Self {
        Self { widget, models }
    }

    /// Register a fixed-text title for `name`.
    pub fn set_static_title(&self, name: &str, title: &str) {
        let window_title = libadwaita::WindowTitle::new(title, "");
        self.widget.add_title(name, &window_title);
    }

    /// Register (and return) a title widget for `name` whose content and
    /// visibility the caller controls (used by detail pages to reveal the
    /// title on scroll).
    pub fn add_title_widget(&self, name: &str) -> libadwaita::WindowTitle {
        let window_title = libadwaita::WindowTitle::new("", "");
        self.widget.add_title(name, &window_title);
        window_title
    }

    /// Register a widget in the optional-buttons (end) area for `name`.
    pub fn add_end(&self, name: &str, widget: &impl IsA<gtk::Widget>) {
        self.widget.add_end(name, widget);
    }

    /// Register a caller-provided widget as the title for `name`.
    pub fn add_title(&self, name: &str, widget: &impl IsA<gtk::Widget>) {
        self.widget.add_title(name, widget);
    }

    /// Register the [`HeaderBarModel`] driving selection/back for `name`.
    pub fn register_model(&self, name: &str, model: Rc<dyn HeaderBarModel>) {
        self.models.borrow_mut().insert(name.to_string(), model);
    }

    /// Remove every contribution registered under `name`.
    pub fn remove(&self, name: &str) {
        self.widget.remove_page(name);
        self.models.borrow_mut().remove(name);
    }
}
