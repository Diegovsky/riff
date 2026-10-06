use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use libadwaita::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/utility_panel.ui")]
    pub struct UtilityPanelWidget {
        #[template_child]
        pub resize_handle: TemplateChild<gtk::Box>,
        #[template_child]
        pub toolbar: TemplateChild<libadwaita::ToolbarView>,
        #[template_child]
        pub title: TemplateChild<libadwaita::WindowTitle>,
        #[template_child]
        pub utility_panel_button: TemplateChild<gtk::ToggleButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for UtilityPanelWidget {
        const NAME: &'static str = "UtilityPanelWidget";
        type Type = super::UtilityPanelWidget;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for UtilityPanelWidget {}
    impl WidgetImpl for UtilityPanelWidget {}
    impl BinImpl for UtilityPanelWidget {}
}

glib::wrapper! {
    pub struct UtilityPanelWidget(ObjectSubclass<imp::UtilityPanelWidget>)
        @extends gtk::Widget, libadwaita::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl UtilityPanelWidget {
    pub fn resize_handle(&self) -> &gtk::Box {
        &self.imp().resize_handle
    }

    /// Show `content` under a header titled `title`.
    pub fn set_content(&self, content: Option<&gtk::Widget>, title: &str) {
        let imp = self.imp();
        imp.toolbar.set_content(content);
        imp.title.set_title(title);
        imp.utility_panel_button.set_tooltip_text(Some(title));
    }
}

pub fn expose_widgets() {
    UtilityPanelWidget::static_type();
}
