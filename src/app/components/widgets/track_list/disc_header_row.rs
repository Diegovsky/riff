use std::cell::RefCell;

use glib::subclass::prelude::*;
use glib::subclass::InitializingObject;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use super::ROW_HEIGHT_PX;
use crate::app::components::utils::set_css_class;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/disc_header_row.ui")]
    pub struct DiscHeaderRow {
        #[template_child]
        pub disc_header_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub header_grid: TemplateChild<gtk::Grid>,
        #[template_child]
        pub disc_header_slot: TemplateChild<gtk::Overlay>,
        #[template_child]
        pub header_button: TemplateChild<gtk::Button>,
        // For the row shown: rows are recycled
        pub on_button: RefCell<Option<Box<dyn Fn()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DiscHeaderRow {
        const NAME: &'static str = "DiscHeaderRow";
        type Type = super::DiscHeaderRow;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DiscHeaderRow {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_height_request(ROW_HEIGHT_PX);
            let row = self.obj().downgrade();
            self.header_button.connect_clicked(move |_| {
                if let Some(row) = row.upgrade() {
                    if let Some(on_button) = row.imp().on_button.borrow().as_ref() {
                        on_button();
                    }
                }
            });
        }
    }
    impl WidgetImpl for DiscHeaderRow {}
    impl BoxImpl for DiscHeaderRow {}
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupButton {
    pub icon_name: &'static str,
    pub tooltip: String,
    pub destructive: bool,
}

glib::wrapper! {
    pub struct DiscHeaderRow(ObjectSubclass<imp::DiscHeaderRow>) @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for DiscHeaderRow {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscHeaderRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_text(&self, text: &str) {
        self.imp().disc_header_label.set_label(text);
    }

    /// A group header has no icon and spans the whole width.
    pub fn set_disc(&self, disc: bool) {
        let imp = self.imp();
        imp.disc_header_slot.set_visible(disc);
        let margin = if disc { 7 } else { 0 };
        imp.header_grid.set_margin_start(margin);
        imp.header_grid.set_margin_end(margin);
        set_css_class(&*imp.disc_header_label, "heading", disc);
        set_css_class(&*imp.disc_header_label, "title-4", !disc);
    }

    pub fn set_button(&self, button: Option<&GroupButton>, on_click: Box<dyn Fn()>) {
        let imp = self.imp();
        let widget = &imp.header_button;
        widget.set_visible(button.is_some());
        // Rows are recycled
        let destructive = button.is_some_and(|b| b.destructive);
        set_css_class(&**widget, "destructive-action", destructive);
        if let Some(button) = button {
            widget.set_icon_name(button.icon_name);
        }
        widget.set_tooltip_text(button.map(|b| b.tooltip.as_str()));
        imp.on_button.replace(button.map(|_| on_click));
    }
}
