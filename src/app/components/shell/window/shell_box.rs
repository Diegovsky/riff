//! Custom root layout for the main window.
//!
//! Stacks content (first child) above fixed-height bottom bars (the mobile
//! now-playing bar and the playback bar). Content absorbs vertical shrink; the
//! bars keep their natural height, pinned to the bottom.
//!
//! A plain `GtkBox` can't do this: forced shorter than its total minimum it
//! clips the last child (the playback controls), the reported bug. `ShellBox`
//! subclasses `GtkWidget` so its `measure`/`size_allocate` are used (a `GtkBox`
//! routes them through `GtkBoxLayout`), and reports only the bars as the
//! vertical minimum so content can shrink to zero without squeezing them.

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct ShellBox;

    #[glib::object_subclass]
    impl ObjectSubclass for ShellBox {
        const NAME: &'static str = "ShellBox";
        type Type = super::ShellBox;
        type ParentType = gtk::Widget;
    }

    impl ShellBox {
        /// Visible children in document order (content first, then bottom bars).
        fn layout_children(&self) -> Vec<gtk::Widget> {
            let mut children = Vec::new();
            let mut child = self.obj().first_child();
            while let Some(widget) = child {
                let next = widget.next_sibling();
                if widget.should_layout() {
                    children.push(widget);
                }
                child = next;
            }
            children
        }
    }

    impl ObjectImpl for ShellBox {
        fn constructed(&self) {
            self.parent_constructed();
            // Clip over-small content; the bars stay pinned within bounds.
            self.obj().set_overflow(gtk::Overflow::Hidden);
        }

        fn dispose(&self) {
            // A plain GtkWidget does not unparent its children automatically.
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for ShellBox {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let children = self.layout_children();
            let Some((content, bars)) = children.split_first() else {
                return (0, 0, -1, -1);
            };

            if orientation == gtk::Orientation::Horizontal {
                let mut min = 0;
                let mut nat = 0;
                for child in &children {
                    let (child_min, child_nat, _, _) = child.measure(orientation, -1);
                    min = min.max(child_min);
                    nat = nat.max(child_nat);
                }
                return (min, nat, -1, -1);
            }

            // Vertical: bars form the whole minimum at natural height; content
            // adds to natural only, so it can be squeezed to zero.
            let mut bars_height = 0;
            for bar in bars {
                let (_, bar_nat, _, _) = bar.measure(orientation, for_size);
                bars_height += bar_nat;
            }
            let (_, content_nat, _, _) = content.measure(orientation, for_size);
            (bars_height, content_nat + bars_height, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let children = self.layout_children();
            let Some((content, bars)) = children.split_first() else {
                return;
            };

            let mut bar_heights = Vec::with_capacity(bars.len());
            let mut bars_height = 0;
            for bar in bars {
                let (_, bar_nat, _, _) = bar.measure(gtk::Orientation::Vertical, width);
                bar_heights.push(bar_nat);
                bars_height += bar_nat;
            }

            // Content fills the space above the bars (clamped at zero).
            let content_height = (height - bars_height).max(0);
            content.allocate(width, content_height, -1, None);

            // Pin bars to the bottom, stacking upward so the last sits flush.
            let mut y = height;
            for (bar, bar_height) in bars.iter().zip(bar_heights.iter()).rev() {
                y -= bar_height;
                let transform =
                    gtk::gsk::Transform::new().translate(&gtk::graphene::Point::new(0.0, y as f32));
                bar.allocate(width, *bar_height, -1, Some(transform));
            }
        }
    }
}

glib::wrapper! {
    pub struct ShellBox(ObjectSubclass<imp::ShellBox>) @extends gtk::Widget;
}

/// Ensure the GObject type is registered so the builder can instantiate it.
pub fn expose_widgets() {
    ShellBox::static_type();
}
