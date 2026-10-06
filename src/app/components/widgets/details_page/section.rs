use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/details_section.ui")]
    pub struct DetailsSectionWidget {
        #[template_child]
        pub header_row: TemplateChild<gtk::Box>,
        #[template_child]
        pub title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub button_slot: TemplateChild<gtk::Box>,
        #[template_child]
        pub empty_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DetailsSectionWidget {
        const NAME: &'static str = "DetailsSectionWidget";
        type Type = super::DetailsSectionWidget;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DetailsSectionWidget {}
    impl WidgetImpl for DetailsSectionWidget {}
    impl BoxImpl for DetailsSectionWidget {}
}

glib::wrapper! {
    /// A titled part of a details page's content (see `details_section.blp`).
    pub struct DetailsSectionWidget(ObjectSubclass<imp::DetailsSectionWidget>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl DetailsSectionWidget {
    pub fn new(title: Option<&str>, content: &impl IsA<gtk::Widget>) -> Self {
        let section: Self = glib::Object::new();
        let imp = section.imp();
        if let Some(title) = title {
            imp.title_label.set_label(title);
            imp.title_label.set_visible(true);
            imp.header_row.set_visible(true);
        }
        section.append(content);
        section
    }

    pub fn header_row(&self) -> &gtk::Box {
        &self.imp().header_row
    }

    pub fn button_slot(&self) -> &gtk::Box {
        &self.imp().button_slot
    }

    pub fn set_button(&self, button: &impl IsA<gtk::Widget>) {
        let imp = self.imp();
        imp.button_slot.append(button);
        imp.header_row.set_visible(true);
    }

    pub fn set_empty(&self, empty: bool) {
        self.imp().empty_label.set_visible(empty);
    }
}
