use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(file = "src/app/components/widgets/card_list/card_list.blp")]
    pub struct CardListWidget {
        #[template_child]
        pub scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub overlay: TemplateChild<gtk::Overlay>,
        #[template_child]
        pub status_page: TemplateChild<libadwaita::StatusPage>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CardListWidget {
        const NAME: &'static str = "CardListWidget";
        type Type = super::CardListWidget;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for CardListWidget {}
    impl WidgetImpl for CardListWidget {}
    impl BoxImpl for CardListWidget {}
}

glib::wrapper! {
    pub struct CardListWidget(ObjectSubclass<imp::CardListWidget>)
        @extends gtk::Widget, gtk::Box;
}

impl CardListWidget {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn scrolled_window(&self) -> &gtk::ScrolledWindow {
        &self.imp().scrolled_window
    }

    pub fn overlay(&self) -> &gtk::Overlay {
        &self.imp().overlay
    }

    pub fn status_page(&self) -> &libadwaita::StatusPage {
        &self.imp().status_page
    }
}
