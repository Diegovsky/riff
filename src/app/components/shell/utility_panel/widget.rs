use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};

use crate::app::components::shell::layout::QUEUE_SHEET_MAX_WIDTH;

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
        pub queue_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub queue_scroller: TemplateChild<gtk::ScrolledWindow>,

        pub in_sheet: Cell<bool>,
        pub queue_list: RefCell<Option<gtk::Widget>>,
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

    impl ObjectImpl for UtilityPanelWidget {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_layout_manager(None::<gtk::LayoutManager>);
        }
    }

    impl WidgetImpl for UtilityPanelWidget {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(child) = self.obj().child() else {
                return (0, 0, -1, -1);
            };
            let (min, natural, min_baseline, natural_baseline) =
                child.measure(orientation, for_size);
            if self.in_sheet.get() && orientation == gtk::Orientation::Horizontal {
                (
                    min,
                    min.max(QUEUE_SHEET_MAX_WIDTH),
                    min_baseline,
                    natural_baseline,
                )
            } else {
                (min, natural, min_baseline, natural_baseline)
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.obj().child() {
                child.allocate(width, height, baseline, None);
            }
        }
    }
    impl BinImpl for UtilityPanelWidget {}
}

glib::wrapper! {
    pub struct UtilityPanelWidget(ObjectSubclass<imp::UtilityPanelWidget>)
        @extends gtk::Widget, libadwaita::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

const LIST_MARGIN: i32 = 12;

/// Room for the bottom sheet's drag handle, drawn over its content
const SHEET_MARGIN_TOP: i32 = 15 + 6 + 15;

/// Inside the scroller, so the scrollbar stays at the sheet's edge
const SHEET_MARGIN_X: i32 = 12;

impl UtilityPanelWidget {
    pub fn resize_handle(&self) -> &gtk::Box {
        &self.imp().resize_handle
    }

    pub fn set_in_sheet(&self, in_sheet: bool) {
        let imp = self.imp();
        imp.in_sheet.set(in_sheet);
        self.queue_resize();
        imp.toolbar.set_reveal_top_bars(!in_sheet);
        imp.queue_stack
            .set_margin_top(if in_sheet { SHEET_MARGIN_TOP } else { 0 });
        self.update_list_margins();
        imp.queue_scroller.set_propagate_natural_height(in_sheet);
    }

    pub fn scroll_to_top(&self) {
        self.imp().queue_scroller.vadjustment().set_value(0.0);
    }

    pub fn set_queue_list(&self, list: &impl IsA<gtk::Widget>) {
        list.set_margin_top(LIST_MARGIN);
        list.set_margin_bottom(LIST_MARGIN);
        let imp = self.imp();
        imp.queue_scroller.set_child(Some(list));
        imp.queue_list.replace(Some(list.clone().upcast()));
        self.update_list_margins();
    }

    fn update_list_margins(&self) {
        let imp = self.imp();
        let Some(list) = imp.queue_list.borrow().clone() else {
            return;
        };
        let margin_x = LIST_MARGIN
            + if imp.in_sheet.get() {
                SHEET_MARGIN_X
            } else {
                0
            };
        list.set_margin_start(margin_x);
        list.set_margin_end(margin_x);
    }

    pub fn set_queue_empty(&self, empty: bool) {
        let page = if empty { "empty" } else { "list" };
        self.imp().queue_stack.set_visible_child_name(page);
    }
}

pub fn expose_widgets() {
    UtilityPanelWidget::static_type();
}
