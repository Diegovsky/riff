use std::rc::Rc;

use gtk::prelude::*;

use super::component::{HeaderBarModel, HeaderModelRegistry};
use super::widget::AppHeaderBar;

#[derive(Clone)]
pub struct HeaderRegistrar {
    widget: AppHeaderBar,
    models: HeaderModelRegistry,
}

impl HeaderRegistrar {
    pub fn new(widget: AppHeaderBar, models: HeaderModelRegistry) -> Self {
        Self { widget, models }
    }

    pub fn page(&self, name: &str) -> PageHeader {
        PageHeader {
            name: name.to_string(),
            widget: self.widget.clone(),
            models: Rc::clone(&self.models),
        }
    }
}

#[derive(Clone)]
pub struct PageHeader {
    name: String,
    widget: AppHeaderBar,
    models: HeaderModelRegistry,
}

impl PageHeader {
    pub fn set_title(&self, title: &str) {
        self.widget
            .update_page(&self.name, |page| page.title = title.to_string());
    }

    pub fn set_subtitle(&self, subtitle: &str) {
        self.widget
            .update_page(&self.name, |page| page.subtitle = subtitle.to_string());
    }

    pub fn set_title_revealed(&self, revealed: bool) {
        self.widget
            .update_page(&self.name, |page| page.title_revealed = revealed);
    }

    pub fn set_title_widget(&self, widget: &impl IsA<gtk::Widget>) {
        let widget = widget.clone().upcast();
        self.widget
            .update_page(&self.name, |page| page.title_widget = Some(widget));
    }

    pub fn add_end(&self, widget: &impl IsA<gtk::Widget>) {
        let widget = widget.clone().upcast();
        self.widget.update_page(&self.name, |page| {
            if !page.end.contains(&widget) {
                page.end.push(widget);
            }
        });
    }

    pub fn remove_end(&self, widget: &impl IsA<gtk::Widget>) {
        let widget = widget.upcast_ref();
        self.widget.detach_end(widget);
        self.widget
            .update_page(&self.name, |page| page.end.retain(|w| w != widget));
    }

    pub fn set_model(&self, model: Rc<dyn HeaderBarModel>) {
        self.models.borrow_mut().insert(self.name.clone(), model);
    }

    pub fn remove(&self) {
        self.widget.remove_page(&self.name);
        self.models.borrow_mut().remove(&self.name);
    }
}
