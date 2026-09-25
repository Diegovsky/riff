use glib::subclass::prelude::*;
use glib::subclass::InitializingObject;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use super::ROW_HEIGHT_PX;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/disc_header_row.ui")]
    pub struct DiscHeaderRow {
        #[template_child]
        pub disc_header_label: TemplateChild<gtk::Label>,
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
        }
    }
    impl WidgetImpl for DiscHeaderRow {}
    impl BoxImpl for DiscHeaderRow {}
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
}
