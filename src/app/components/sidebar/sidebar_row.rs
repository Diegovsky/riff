use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use super::SidebarItem;

impl SidebarRow {
    pub fn new(item: SidebarItem) -> Self {
        glib::Object::builder().property("item", item).build()
    }
}

mod imp {
    use super::*;
    use glib::Properties;
    use std::cell::RefCell;

    #[derive(Debug, CompositeTemplate, Properties)]
    #[template(resource = "/dev/diegovsky/Riff/sidebar/sidebar_row.ui")]
    #[properties(wrapper_type = super::SidebarRow)]
    pub struct SidebarRow {
        #[template_child]
        pub icon: TemplateChild<gtk::Image>,

        #[template_child]
        pub title: TemplateChild<gtk::Label>,

        #[property(get, set = Self::set_item)]
        pub item: RefCell<SidebarItem>,
    }

    impl SidebarRow {
        fn set_item(&self, item: SidebarItem) {
            self.title.set_text(item.title().as_str());
            self.icon.set_icon_name(item.icon());
            self.obj().set_tooltip_text(Some(item.title().as_str()));
            self.item.replace(item);
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SidebarRow {
        const NAME: &'static str = "SidebarRow";
        type Type = super::SidebarRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }

        fn new() -> Self {
            Self {
                icon: Default::default(),
                title: Default::default(),
                item: RefCell::new(glib::Object::new()),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for SidebarRow {}
    impl WidgetImpl for SidebarRow {}
    impl ListBoxRowImpl for SidebarRow {}
}

glib::wrapper! {
    pub struct SidebarRow(ObjectSubclass<imp::SidebarRow>) @extends gtk::Widget, gtk::ListBoxRow;
}
