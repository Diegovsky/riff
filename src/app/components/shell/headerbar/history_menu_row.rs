use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use super::HistoryMenuItem;

impl HistoryMenuRow {
    pub fn new(item: &HistoryMenuItem) -> Self {
        let row: Self = glib::Object::new();
        let imp = row.imp();
        imp.entry_id.set(item.id);
        imp.icon.set_icon_name(Some(item.icon));
        imp.label.set_text(&item.label);
        row
    }

    pub fn entry_id(&self) -> u64 {
        self.imp().entry_id.get()
    }
}

mod imp {
    use super::*;
    use std::cell::Cell;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/history_menu_row.ui")]
    pub struct HistoryMenuRow {
        #[template_child]
        pub icon: TemplateChild<gtk::Image>,

        #[template_child]
        pub label: TemplateChild<gtk::Label>,

        pub entry_id: Cell<u64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for HistoryMenuRow {
        const NAME: &'static str = "HistoryMenuRow";
        type Type = super::HistoryMenuRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for HistoryMenuRow {}
    impl WidgetImpl for HistoryMenuRow {}
    impl ListBoxRowImpl for HistoryMenuRow {}
}

glib::wrapper! {
    pub struct HistoryMenuRow(ObjectSubclass<imp::HistoryMenuRow>) @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}
