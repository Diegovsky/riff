use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use libadwaita::subclass::prelude::BinImpl;

use crate::app::components::labels;

mod imp {

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/headerbar.ui")]
    pub struct AppHeaderBar {
        #[template_child]
        pub sidebar_toggle: TemplateChild<gtk::ToggleButton>,

        #[template_child]
        pub go_back: TemplateChild<gtk::Button>,

        #[template_child]
        pub title_stack: TemplateChild<gtk::Stack>,

        #[template_child]
        pub end_stack: TemplateChild<gtk::Stack>,

        #[template_child]
        pub start_selection: TemplateChild<gtk::Button>,

        #[template_child]
        pub selection_header: TemplateChild<libadwaita::HeaderBar>,

        #[template_child]
        pub selection_title: TemplateChild<libadwaita::WindowTitle>,

        #[template_child]
        pub select_all: TemplateChild<gtk::Button>,

        #[template_child]
        pub cancel: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AppHeaderBar {
        const NAME: &'static str = "AppHeaderBar";
        type Type = super::AppHeaderBar;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AppHeaderBar {}
    impl WidgetImpl for AppHeaderBar {}
    impl BinImpl for AppHeaderBar {}
}

glib::wrapper! {
    pub struct AppHeaderBar(ObjectSubclass<imp::AppHeaderBar>) @extends gtk::Widget, libadwaita::Bin;
}

impl Default for AppHeaderBar {
    fn default() -> Self {
        Self::new()
    }
}

impl AppHeaderBar {
    pub fn new() -> Self {
        glib::Object::new()
    }

    // --- Per-screen page registration ---

    /// Register a title-area widget for `name`. No-op if one already exists.
    pub fn add_title(&self, name: &str, widget: &impl IsA<gtk::Widget>) {
        let stack = &self.imp().title_stack;
        if stack.child_by_name(name).is_none() {
            stack.add_named(widget, Some(name));
        }
    }

    /// Register an end-area widget for `name`. No-op if one already exists.
    pub fn add_end(&self, name: &str, widget: &impl IsA<gtk::Widget>) {
        let stack = &self.imp().end_stack;
        if stack.child_by_name(name).is_none() {
            stack.add_named(widget, Some(name));
        }
    }

    /// Remove any title and end pages registered under `name`.
    pub fn remove_page(&self, name: &str) {
        if let Some(child) = self.imp().title_stack.child_by_name(name) {
            self.imp().title_stack.remove(&child);
        }
        if let Some(child) = self.imp().end_stack.child_by_name(name) {
            self.imp().end_stack.remove(&child);
        }
    }

    /// Show the title and end pages for `name`; hide the end area if none.
    pub fn set_active(&self, name: &str) {
        let imp = self.imp();
        if imp.title_stack.child_by_name(name).is_some() {
            imp.title_stack.set_visible_child_name(name);
        }
        if imp.end_stack.child_by_name(name).is_some() {
            imp.end_stack.set_visible_child_name(name);
            imp.end_stack.set_visible(true);
        } else {
            imp.end_stack.set_visible(false);
        }
    }

    // --- Fixed control signals ---

    pub fn connect_go_back<F: Fn() + 'static>(&self, f: F) {
        self.imp().go_back.connect_clicked(move |_| f());
    }

    pub fn connect_selection_start<F: Fn() + 'static>(&self, f: F) {
        self.imp().start_selection.connect_clicked(move |_| f());
    }

    pub fn connect_select_all<F: Fn() + 'static>(&self, f: F) {
        self.imp().select_all.connect_clicked(move |_| f());
    }

    pub fn connect_selection_cancel<F: Fn() + 'static>(&self, f: F) {
        self.imp().cancel.connect_clicked(move |_| f());
    }

    // --- Fixed control state ---

    pub fn set_can_go_back(&self, can_go_back: bool) {
        self.imp().go_back.set_visible(can_go_back);
    }

    pub fn set_selection_possible(&self, possible: bool) {
        self.imp().start_selection.set_visible(possible);
    }

    pub fn set_select_all_possible(&self, possible: bool) {
        self.imp().select_all.set_visible(possible);
    }

    pub fn set_selection_active(&self, active: bool) {
        let imp = self.imp();
        if active {
            imp.selection_title
                .set_title(&labels::n_tracks_selected_label(0));
            imp.selection_title.set_visible(true);
            imp.selection_header.set_visible(true);
        } else {
            imp.selection_title.set_visible(false);
            imp.selection_header.set_visible(false);
        }
    }

    pub fn set_selection_count(&self, count: usize) {
        self.imp()
            .selection_title
            .set_title(&labels::n_tracks_selected_label(count));
    }
}
