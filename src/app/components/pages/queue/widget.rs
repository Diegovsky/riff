use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};

use crate::app::components::labels;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/queue_page.ui")]
    pub struct QueuePageWidget {
        #[template_child]
        pub selection_bar: TemplateChild<gtk::CenterBox>,
        #[template_child]
        pub selection_cancel: TemplateChild<gtk::Button>,
        #[template_child]
        pub selection_count: TemplateChild<gtk::Label>,
        #[template_child]
        pub select_all: TemplateChild<gtk::Button>,
        #[template_child]
        pub queue_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub queue_scroller: TemplateChild<gtk::ScrolledWindow>,

        pub queue_list: RefCell<Option<gtk::Widget>>,
        pub extra_margin_x: Cell<i32>,
        pub selection_bar_hidden: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for QueuePageWidget {
        const NAME: &'static str = "QueuePageWidget";
        type Type = super::QueuePageWidget;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for QueuePageWidget {}
    impl WidgetImpl for QueuePageWidget {}
    impl BinImpl for QueuePageWidget {}
}

glib::wrapper! {
    pub struct QueuePageWidget(ObjectSubclass<imp::QueuePageWidget>)
        @extends gtk::Widget, libadwaita::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

const LIST_MARGIN: i32 = 12;

impl Default for QueuePageWidget {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl QueuePageWidget {
    pub fn set_queue_list(&self, list: &impl IsA<gtk::Widget>) {
        list.set_margin_top(LIST_MARGIN);
        list.set_margin_bottom(LIST_MARGIN);
        let imp = self.imp();
        imp.queue_scroller.set_child(Some(list));
        imp.queue_list.replace(Some(list.clone().upcast()));
        self.update_list_margins();
    }

    pub fn set_queue_empty(&self, empty: bool) {
        let page = if empty { "empty" } else { "list" };
        self.imp().queue_stack.set_visible_child_name(page);
    }

    pub fn connect_selection_cancel<F: Fn() + 'static>(&self, f: F) {
        self.imp().selection_cancel.connect_clicked(move |_| f());
    }

    pub fn connect_select_all<F: Fn() + 'static>(&self, f: F) {
        self.imp().select_all.connect_clicked(move |_| f());
    }

    pub fn set_selection_bar_hidden(&self, hidden: bool) {
        let imp = self.imp();
        imp.selection_bar_hidden.set(hidden);
        if hidden {
            imp.selection_bar.set_visible(false);
        }
    }

    pub fn set_selection(&self, selected: Option<usize>, all_selected: bool) {
        let imp = self.imp();
        imp.selection_bar
            .set_visible(selected.is_some() && !imp.selection_bar_hidden.get());
        let Some(count) = selected else {
            return;
        };
        imp.selection_count
            .set_label(&labels::n_tracks_selected_label(count));
        imp.select_all.set_label(&if all_selected {
            // translators: Button label. Deselects all songs, once they are all selected.
            gettext("Deselect All")
        } else {
            // translators: Button label. Selects all visible songs.
            gettext("Select All")
        });
    }

    pub fn scroll_to_top(&self) {
        self.imp().queue_scroller.vadjustment().set_value(0.0);
    }

    pub fn set_extra_margin_x(&self, margin: i32) {
        self.imp().extra_margin_x.set(margin);
        self.update_list_margins();
    }

    pub fn set_fit_content_height(&self, fit: bool) {
        self.imp().queue_scroller.set_propagate_natural_height(fit);
    }

    fn update_list_margins(&self) {
        let imp = self.imp();
        let Some(list) = imp.queue_list.borrow().clone() else {
            return;
        };
        let margin_x = LIST_MARGIN + imp.extra_margin_x.get();
        list.set_margin_start(margin_x);
        list.set_margin_end(margin_x);
    }
}
