use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

use crate::app::components::labels;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/utility_panel.ui")]
    pub struct UtilityPanelWidget {
        #[template_child]
        pub resize_handle: TemplateChild<gtk::Box>,
        #[template_child]
        pub content: TemplateChild<libadwaita::Bin>,
        #[template_child]
        pub header: TemplateChild<libadwaita::HeaderBar>,
        #[template_child]
        pub title: TemplateChild<libadwaita::WindowTitle>,
        #[template_child]
        pub selection_header: TemplateChild<libadwaita::HeaderBar>,
        #[template_child]
        pub selection_title: TemplateChild<libadwaita::WindowTitle>,
        #[template_child]
        pub selection_cancel: TemplateChild<gtk::Button>,
        #[template_child]
        pub select_all: TemplateChild<gtk::Button>,
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
        imp.content.set_child(content);
        imp.title.set_title(title);
        imp.utility_panel_button.set_tooltip_text(Some(title));
    }

    pub fn connect_selection_cancel<F: Fn() + 'static>(&self, f: F) {
        self.imp().selection_cancel.connect_clicked(move |_| f());
    }

    pub fn connect_select_all<F: Fn() + 'static>(&self, f: F) {
        self.imp().select_all.connect_clicked(move |_| f());
    }

    pub fn set_selection(&self, selected: Option<usize>, all_selected: bool) {
        let imp = self.imp();
        imp.header.set_visible(selected.is_none());
        imp.selection_header.set_visible(selected.is_some());
        let Some(count) = selected else {
            return;
        };
        imp.selection_title
            .set_title(&labels::n_tracks_selected_label(count));
        imp.select_all.set_label(&if all_selected {
            // translators: Button label. Deselects all songs, once they are all selected.
            gettext("Deselect All")
        } else {
            // translators: Button label. Selects all visible songs.
            gettext("Select All")
        });
    }
}

pub fn expose_widgets() {
    UtilityPanelWidget::static_type();
}
