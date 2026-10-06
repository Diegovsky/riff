use gtk::prelude::*;
use std::rc::Rc;

use super::UtilityPanelWidget;
use crate::app::components::WindowLayout;

/// The panel beside the content, at the window's end. Whatever it shows is
/// set with [`UtilityPanel::set_content`]; without content it leaves the
/// split view, so it can't be opened.
#[derive(Clone)]
pub struct UtilityPanel {
    widget: UtilityPanelWidget,
    split: libadwaita::OverlaySplitView,
    layout: Rc<WindowLayout>,
}

impl UtilityPanel {
    pub fn new(
        widget: UtilityPanelWidget,
        split: libadwaita::OverlaySplitView,
        layout: Rc<WindowLayout>,
    ) -> Self {
        let panel = Self {
            widget,
            split,
            layout,
        };
        panel.set_content(None, "");
        panel
    }

    pub fn set_content(&self, content: Option<&gtk::Widget>, title: &str) {
        self.widget.set_content(content, title);
        self.split
            .set_sidebar(content.is_some().then_some(&self.widget));
        self.widget
            .resize_handle()
            .set_visible(content.is_some() && !self.split.is_collapsed());
    }

    pub fn is_open(&self) -> bool {
        self.split.shows_sidebar()
    }

    pub fn set_open(&self, open: bool) {
        self.layout.set_utility_open(open);
    }

    pub fn connect_open_notify<F: Fn() + 'static>(&self, f: F) {
        self.split.connect_show_sidebar_notify(move |_| f());
    }
}
