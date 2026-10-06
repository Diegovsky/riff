use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

use crate::app::components::shell::layout::BOTTOM_SHEET_MAX_WIDTH;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/bottom_sheet.ui")]
    pub struct BottomSheetContent;

    #[glib::object_subclass]
    impl ObjectSubclass for BottomSheetContent {
        const NAME: &'static str = "BottomSheetContent";
        type Type = super::BottomSheetContent;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for BottomSheetContent {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_layout_manager(None::<gtk::LayoutManager>);
        }
    }

    impl WidgetImpl for BottomSheetContent {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(child) = self.obj().child() else {
                return (0, 0, -1, -1);
            };
            let (min, natural, min_baseline, natural_baseline) =
                child.measure(orientation, for_size);
            match orientation {
                gtk::Orientation::Horizontal => (
                    min,
                    min.max(BOTTOM_SHEET_MAX_WIDTH),
                    min_baseline,
                    natural_baseline,
                ),
                _ => (min, natural, min_baseline, natural_baseline),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.obj().child() {
                child.allocate(width, height, baseline, None);
            }
        }
    }

    impl BinImpl for BottomSheetContent {}
}

glib::wrapper! {
    pub struct BottomSheetContent(ObjectSubclass<imp::BottomSheetContent>)
        @extends gtk::Widget, libadwaita::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for BottomSheetContent {
    fn default() -> Self {
        glib::Object::new()
    }
}
