use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use libadwaita::subclass::prelude::BinImpl;

mod imp {

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/alextren/Spot/components/playlist_headerbar.ui")]
    pub struct PlaylistHeaderBarWidget {
        #[template_child]
        pub main_header: TemplateChild<libadwaita::HeaderBar>,

        #[template_child]
        pub edition_header: TemplateChild<libadwaita::HeaderBar>,

        #[template_child]
        pub go_back: TemplateChild<gtk::Button>,

        #[template_child]
        pub title: TemplateChild<libadwaita::WindowTitle>,

        #[template_child]
        pub edit: TemplateChild<gtk::Button>,

        #[template_child]
        pub ok: TemplateChild<gtk::Button>,

        #[template_child]
        pub cancel: TemplateChild<gtk::Button>,

        #[template_child]
        pub overlay: TemplateChild<gtk::Overlay>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlaylistHeaderBarWidget {
        const NAME: &'static str = "PlaylistHeaderBarWidget";
        type Type = super::PlaylistHeaderBarWidget;
        type ParentType = libadwaita::Bin;
        type Interfaces = (gtk::Buildable,);

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PlaylistHeaderBarWidget {}

    impl BuildableImpl for PlaylistHeaderBarWidget {
        fn add_child(&self, builder: &gtk::Builder, child: &glib::Object, type_: Option<&str>) {
            if Some("root") == type_ {
                self.parent_add_child(builder, child, type_);
            } else {
                self.main_header
                    .set_title_widget(child.downcast_ref::<gtk::Widget>());
            }
        }
    }

    impl WidgetImpl for PlaylistHeaderBarWidget {}
    impl BinImpl for PlaylistHeaderBarWidget {}
    impl WindowImpl for PlaylistHeaderBarWidget {}
}

glib::wrapper! {
    pub struct PlaylistHeaderBarWidget(ObjectSubclass<imp::PlaylistHeaderBarWidget>) @extends gtk::Widget, libadwaita::Bin;
}

impl Default for PlaylistHeaderBarWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaylistHeaderBarWidget {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_edit<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp().edit.connect_clicked(move |_| f());
    }

    pub fn connect_ok<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp().ok.connect_clicked(move |_| f());
    }

    pub fn connect_cancel<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp().cancel.connect_clicked(move |_| f());
    }

    pub fn connect_go_back<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp().go_back.connect_clicked(move |_| f());
    }

    pub fn set_can_go_back(&self, can_go_back: bool) {
        self.imp().go_back.set_visible(can_go_back);
    }

    pub fn set_editable(&self, editable: bool) {
        self.imp().edit.set_visible(editable);
    }

    pub fn set_editing(&self, editing: bool) {
        if editing {
            self.imp().edition_header.set_visible(true);
        } else {
            self.imp().edition_header.set_visible(false);
        }
    }

    pub fn add_classes(&self, classes: &[&str]) {
        for &class in classes {
            self.add_css_class(class);
        }
    }

    pub fn remove_classes(&self, classes: &[&str]) {
        for &class in classes {
            self.remove_css_class(class);
        }
    }

    pub fn set_title_visible(&self, visible: bool) {
        self.imp().title.set_visible(visible);
    }

    pub fn set_title(&self, title: Option<&str>) {
        self.imp().title.set_visible(title.is_some());
        if let Some(title) = title {
            self.imp().title.set_title(title);
        }
    }
}
