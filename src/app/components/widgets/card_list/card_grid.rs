//! The grid behind a `CardList`: a `FlowBox` in a wrapper that can align its
//! cells edge to edge while allocating, so no frame shows a stale layout.

use gtk::prelude::*;
use gtk::subclass::prelude::*;

/// Only decides how many columns fit; fits three small cards at minimum width.
const ALIGNED_COLUMN_SPACING: u32 = 2;

fn fitting_columns(width: i32, cell: i32, spacing: i32, max: i32) -> i32 {
    ((width + spacing) / (cell + spacing)).clamp(1, max.max(1))
}

fn aligned_x(column: i32, columns: i32, width: i32, cell: i32) -> i32 {
    column * (width - cell) / (columns - 1)
}

mod imp {
    use super::*;
    use std::cell::Cell;

    pub struct CardGrid {
        pub flowbox: gtk::FlowBox,
        pub align_to_edges: Cell<bool>,
    }

    impl Default for CardGrid {
        fn default() -> Self {
            Self {
                flowbox: gtk::FlowBox::new(),
                align_to_edges: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CardGrid {
        const NAME: &'static str = "CardGrid";
        type Type = super::CardGrid;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for CardGrid {
        fn constructed(&self) {
            self.parent_constructed();
            self.flowbox.set_parent(&*self.obj());
        }

        fn dispose(&self) {
            self.flowbox.unparent();
        }
    }

    impl WidgetImpl for CardGrid {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            self.flowbox.request_mode()
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            self.flowbox.measure(orientation, for_size)
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.flowbox.allocate(width, height, baseline, None);
            if self.align_to_edges.get() {
                self.align_cells(width);
            }
        }
    }

    impl CardGrid {
        /// Moves the cells `FlowBox` just allocated, which are stretched to
        /// share the row, to their natural width at edge-aligned positions.
        fn align_cells(&self, width: i32) {
            let grid = &self.flowbox;
            let Some(first) = grid.child_at_index(0) else {
                return;
            };
            let (cell_min, cell, _, _) = first.measure(gtk::Orientation::Horizontal, -1);
            if cell_min != cell || cell <= 0 {
                return;
            }
            let spacing = grid.column_spacing() as i32;
            let max = grid.max_children_per_line() as i32;
            let columns = fitting_columns(width, cell, spacing, max);
            if columns < 2 {
                return;
            }

            let stride = (width + spacing) as f32 / columns as f32;
            let mut child = grid.first_child();
            while let Some(c) = child {
                child = c.next_sibling();
                if !c.should_layout() {
                    continue;
                }
                let Some(bounds) = c.compute_bounds(grid) else {
                    continue;
                };
                let column = ((bounds.x() / stride).round() as i32).clamp(0, columns - 1);
                let x = aligned_x(column, columns, width, cell);
                let transform = gtk::gsk::Transform::new()
                    .translate(&gtk::graphene::Point::new(x as f32, bounds.y()));
                c.allocate(cell, bounds.height().round() as i32, -1, Some(transform));
            }
        }
    }
}

glib::wrapper! {
    pub struct CardGrid(ObjectSubclass<imp::CardGrid>) @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl CardGrid {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn flowbox(&self) -> &gtk::FlowBox {
        &self.imp().flowbox
    }

    fn set_align_to_edges(&self) {
        self.imp().align_to_edges.set(true);
        self.flowbox().set_column_spacing(ALIGNED_COLUMN_SPACING);
    }
}

pub fn align_cards_to_edges(grid: &gtk::Widget) {
    if let Some(grid) = grid.downcast_ref::<CardGrid>() {
        grid.set_align_to_edges();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL_CELL: i32 = 108;
    const NARROWEST_GRID: i32 = 330;

    #[test]
    fn three_small_cards_fit_at_the_minimum_window_width() {
        let spacing = ALIGNED_COLUMN_SPACING as i32;
        assert_eq!(fitting_columns(NARROWEST_GRID, SMALL_CELL, spacing, 24), 3);
    }

    #[test]
    fn columns_stay_within_the_grid_limits() {
        assert_eq!(fitting_columns(50, SMALL_CELL, 2, 24), 1);
        assert_eq!(fitting_columns(5000, SMALL_CELL, 2, 24), 24);
    }

    #[test]
    fn outer_cells_touch_the_edges() {
        assert_eq!(aligned_x(0, 3, NARROWEST_GRID, SMALL_CELL), 0);
        assert_eq!(aligned_x(1, 3, NARROWEST_GRID, SMALL_CELL), 111);
        assert_eq!(
            aligned_x(2, 3, NARROWEST_GRID, SMALL_CELL),
            NARROWEST_GRID - SMALL_CELL
        );
    }
}
