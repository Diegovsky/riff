//! A bin with no minimum width, which clips its child instead: otherwise an
//! `Adw.OverlaySplitView` squeezes a side panel sliding in.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct ClipBin;

    #[glib::object_subclass]
    impl ObjectSubclass for ClipBin {
        const NAME: &'static str = "ClipBin";
        type Type = super::ClipBin;
        type ParentType = libadwaita::Bin;
    }

    impl ObjectImpl for ClipBin {
        fn constructed(&self) {
            self.parent_constructed();
            // Else AdwBin's layout manager would size it
            self.obj().set_layout_manager(None::<gtk::LayoutManager>);
            self.obj().set_overflow(gtk::Overflow::Hidden);
        }
    }

    impl WidgetImpl for ClipBin {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(child) = self.obj().child() else {
                return (0, 0, -1, -1);
            };
            match orientation {
                gtk::Orientation::Horizontal => {
                    let (_, natural, _, _) = child.measure(orientation, for_size);
                    (0, natural, -1, -1)
                }
                _ => {
                    let for_size = if for_size < 0 {
                        for_size
                    } else {
                        for_size.max(child_min_width(&child))
                    };
                    child.measure(orientation, for_size)
                }
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.obj().child() {
                child.allocate(width.max(child_min_width(&child)), height, baseline, None);
            }
        }
    }

    impl BinImpl for ClipBin {}

    fn child_min_width(child: &gtk::Widget) -> i32 {
        child.measure(gtk::Orientation::Horizontal, -1).0
    }
}

glib::wrapper! {
    pub struct ClipBin(ObjectSubclass<imp::ClipBin>)
        @extends gtk::Widget, libadwaita::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

pub fn expose_widgets() {
    ClipBin::static_type();
}
