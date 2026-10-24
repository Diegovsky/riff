use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use super::{NavigationPanelDestination, NavigationPanelItem};

impl NavigationPanelRow {
    pub fn new(item: NavigationPanelItem) -> Self {
        glib::Object::builder().property("item", item).build()
    }
}

mod imp {
    use super::*;
    use glib::Properties;
    use std::cell::RefCell;

    #[derive(Debug, CompositeTemplate, Properties)]
    #[template(resource = "/dev/diegovsky/Riff/navigation_panel/navigation_panel_row.ui")]
    #[properties(wrapper_type = super::NavigationPanelRow)]
    pub struct NavigationPanelRow {
        #[template_child]
        pub icon: TemplateChild<gtk::Image>,

        #[template_child]
        pub now_playing_icon: TemplateChild<gtk::Overlay>,

        #[template_child]
        pub title: TemplateChild<gtk::Label>,

        #[property(get, set = Self::set_item)]
        pub item: RefCell<NavigationPanelItem>,
    }

    impl NavigationPanelRow {
        fn set_item(&self, item: NavigationPanelItem) {
            self.title.set_text(item.title().as_str());
            self.icon.set_icon_name(item.icon());
            let now_playing = matches!(
                item.destination(),
                Some(NavigationPanelDestination::NowPlaying)
            );
            self.icon.set_visible(!now_playing);
            self.now_playing_icon.set_visible(now_playing);
            self.obj().set_tooltip_text(Some(item.title().as_str()));
            self.item.replace(item);
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NavigationPanelRow {
        const NAME: &'static str = "NavigationPanelRow";
        type Type = super::NavigationPanelRow;
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
                now_playing_icon: Default::default(),
                title: Default::default(),
                item: RefCell::new(glib::Object::new()),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for NavigationPanelRow {}
    impl WidgetImpl for NavigationPanelRow {}
    impl ListBoxRowImpl for NavigationPanelRow {}
}

glib::wrapper! {
    pub struct NavigationPanelRow(ObjectSubclass<imp::NavigationPanelRow>) @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}
