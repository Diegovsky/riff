use gtk::prelude::*;
use gtk::subclass::prelude::*;

// GObject widget

mod imp {
    use super::*;
    use std::cell::RefCell;

    /// Horizontal container for the subtitle artist links.
    ///
    /// Lays its children out left to right at their natural width. When the
    /// combined width exceeds the available space, the children that do not fit
    /// are hidden and a trailing ellipsis ("…") is shown in their place, so the
    /// row always respects the page width instead of overflowing.
    #[derive(Debug, Default)]
    pub struct SubtitleLinksBox {
        /// Persistent trailing label shown only when children are clipped.
        pub ellipsis: RefCell<Option<gtk::Label>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SubtitleLinksBox {
        const NAME: &'static str = "SubtitleLinksBox";
        type Type = super::SubtitleLinksBox;
        type ParentType = gtk::Widget;
    }

    impl SubtitleLinksBox {
        /// The link children in order, excluding the managed ellipsis label.
        fn link_children(&self) -> Vec<gtk::Widget> {
            let ellipsis = self.ellipsis.borrow();
            let ellipsis = ellipsis.as_ref().map(|e| e.upcast_ref::<gtk::Widget>());
            let mut children = Vec::new();
            let mut next = self.obj().first_child();
            while let Some(child) = next {
                next = child.next_sibling();
                if Some(&child) != ellipsis {
                    children.push(child);
                }
            }
            children
        }
    }

    impl ObjectImpl for SubtitleLinksBox {
        fn constructed(&self) {
            self.parent_constructed();

            // The ellipsis is kept as the last child so links can be inserted
            // before it without reordering.
            let ellipsis = gtk::Label::new(Some("…"));
            ellipsis.add_css_class("body");
            ellipsis.set_child_visible(false);
            ellipsis.set_parent(&*self.obj());
            *self.ellipsis.borrow_mut() = Some(ellipsis);
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for SubtitleLinksBox {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let links = self.link_children();
            let ellipsis = self.ellipsis.borrow();
            let ellipsis = ellipsis.as_ref().expect("ellipsis label");

            if orientation == gtk::Orientation::Horizontal {
                // Natural width fits every link; minimum only needs to fit the
                // ellipsis so the row can shrink as far as the page requires.
                let natural: i32 = links
                    .iter()
                    .map(|c| c.measure(orientation, for_size).1)
                    .sum();
                let minimum = ellipsis.measure(orientation, for_size).1;
                (minimum.min(natural), natural.max(minimum), -1, -1)
            } else {
                // Height is the tallest child, including the ellipsis.
                let mut minimum = ellipsis.measure(orientation, for_size).0;
                let mut natural = ellipsis.measure(orientation, for_size).1;
                for child in &links {
                    let (child_min, child_nat, _, _) = child.measure(orientation, for_size);
                    minimum = minimum.max(child_min);
                    natural = natural.max(child_nat);
                }
                (minimum, natural, -1, -1)
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let links = self.link_children();
            let ellipsis = self.ellipsis.borrow();
            let ellipsis = ellipsis.as_ref().expect("ellipsis label");

            let link_widths: Vec<i32> = links
                .iter()
                .map(|c| c.measure(gtk::Orientation::Horizontal, height).1)
                .collect();
            let total: i32 = link_widths.iter().sum();

            let allocate = |child: &gtk::Widget, x: i32, w: i32| {
                child.set_child_visible(true);
                child.size_allocate(&gtk::Allocation::new(x, 0, w, height), baseline);
            };

            // Everything fits: lay out every link and hide the ellipsis.
            if total <= width {
                let mut x = 0;
                for (child, w) in links.iter().zip(&link_widths) {
                    allocate(child, x, *w);
                    x += w;
                }
                ellipsis.set_child_visible(false);
                return;
            }

            // Otherwise, place links until the next one (plus room for the
            // ellipsis) would overflow, then clip the rest.
            let ellipsis_width = ellipsis.measure(gtk::Orientation::Horizontal, height).1;
            let count = links.len();
            let mut x = 0;
            let mut clipped_from = count;
            for (i, (child, w)) in links.iter().zip(&link_widths).enumerate() {
                let is_last = i == count - 1;
                let reserve = if is_last { 0 } else { ellipsis_width };
                if x + w + reserve <= width {
                    allocate(child, x, *w);
                    x += w;
                } else {
                    clipped_from = i;
                    break;
                }
            }

            for child in &links[clipped_from..] {
                child.set_child_visible(false);
            }

            if clipped_from < count {
                let remaining = (width - x).max(0);
                ellipsis.set_child_visible(true);
                ellipsis.size_allocate(
                    &gtk::Allocation::new(x, 0, ellipsis_width.min(remaining), height),
                    baseline,
                );
            } else {
                ellipsis.set_child_visible(false);
            }
        }
    }
}

glib::wrapper! {
    pub struct SubtitleLinksBox(ObjectSubclass<imp::SubtitleLinksBox>) @extends gtk::Widget;
}

impl SubtitleLinksBox {
    /// Append a link widget, keeping it before the trailing ellipsis label.
    pub fn append_link(&self, child: &impl IsA<gtk::Widget>) {
        let imp = self.imp();
        let ellipsis = imp.ellipsis.borrow();
        child.as_ref().insert_before(
            self,
            ellipsis.as_ref().map(|e| e.upcast_ref::<gtk::Widget>()),
        );
    }

    /// Remove all link widgets, preserving the managed ellipsis label.
    pub fn clear_links(&self) {
        let imp = self.imp();
        let ellipsis = imp
            .ellipsis
            .borrow()
            .as_ref()
            .map(|e| e.clone().upcast::<gtk::Widget>());
        let mut next = self.first_child();
        while let Some(child) = next {
            next = child.next_sibling();
            if Some(&child) != ellipsis.as_ref() {
                child.unparent();
            }
        }
    }
}

impl Default for SubtitleLinksBox {
    fn default() -> Self {
        glib::Object::new()
    }
}

/// Ensure the GObject type is registered (called at app startup).
pub fn expose_widgets() {
    SubtitleLinksBox::static_type();
}
