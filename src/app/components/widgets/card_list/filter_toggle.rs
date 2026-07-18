use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use std::cell::Cell;
use std::rc::Rc;

use super::widget::CardList;
use crate::app::models::FilterOption;

// GObject subclass for the BLP template

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(file = "src/app/components/widgets/card_list/filter_toggle.blp")]
    pub struct FilterToggleWidget {
        #[template_child]
        pub wide_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub menu_button: TemplateChild<gtk::MenuButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FilterToggleWidget {
        const NAME: &'static str = "FilterToggleWidget";
        type Type = super::FilterToggleWidget;
        type ParentType = libadwaita::BreakpointBin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for FilterToggleWidget {}
    impl WidgetImpl for FilterToggleWidget {}
    impl libadwaita::subclass::prelude::BreakpointBinImpl for FilterToggleWidget {}
}

glib::wrapper! {
    pub struct FilterToggleWidget(ObjectSubclass<imp::FilterToggleWidget>)
        @extends gtk::Widget, libadwaita::BreakpointBin;
}

impl FilterToggleWidget {
    fn new() -> Self {
        glib::Object::new()
    }
}

/// Shared state for synchronizing wide/narrow mode button groups.
struct FilterState {
    active_index: Rc<Cell<usize>>,
    syncing: Rc<Cell<bool>>,
    card_list: Rc<CardList>,
    on_changed: Rc<dyn Fn(&str, usize)>,
}

impl FilterState {
    /// Apply a filter selection. Called from both wide and narrow button handlers.
    fn apply(&self, index: usize, category: &str) {
        self.active_index.set(index);
        self.card_list.set_filter(category);
        let count = self.card_list.visible_count();
        (self.on_changed)(category, count);
    }

    fn is_syncing(&self) -> bool {
        self.syncing.get()
    }
}

/// A responsive filter control for card lists.
///
/// Uses `Adw.BreakpointBin` to automatically switch between:
/// - **Wide mode**: a segmented toggle button bar (linked `ToggleButton`s)
/// - **Narrow mode**: a `MenuButton` labeled "Filter" with a popover of radio buttons
///
/// The breakpoint condition (`max-width: 360sp`) handles responsive switching
/// automatically based on the available width.
///
/// After construction, only the returned `FilterToggleWidget` needs to be kept alive
/// (by inserting it into the widget tree). All state is owned by GTK signal closures.
pub struct FilterToggle;

impl FilterToggle {
    /// Create a new responsive filter toggle widget.
    ///
    /// - `options`: the available filter choices (first is active by default).
    /// - `card_list`: the card list to filter.
    /// - `on_changed`: called after the filter changes with the new category and
    ///   the number of visible (non-placeholder) cards remaining.
    ///
    /// Returns the widget to insert into the UI.
    pub fn new(
        options: &[FilterOption],
        card_list: Rc<CardList>,
        on_changed: impl Fn(&str, usize) + 'static,
    ) -> FilterToggleWidget {
        let widget = FilterToggleWidget::new();
        let imp = widget.imp();

        let state = Rc::new(FilterState {
            active_index: Rc::new(Cell::new(0)),
            syncing: Rc::new(Cell::new(false)),
            card_list,
            on_changed: Rc::new(on_changed),
        });

        // --- Wide mode: linked ToggleButtons ---
        let wide_box = &*imp.wide_box;
        let mut first_btn: Option<gtk::ToggleButton> = None;
        let mut toggle_buttons: Vec<gtk::ToggleButton> = Vec::with_capacity(options.len());

        for (i, option) in options.iter().enumerate() {
            let btn = gtk::ToggleButton::with_label(&option.label);
            if let Some(ref group) = first_btn {
                btn.set_group(Some(group));
            } else {
                first_btn = Some(btn.clone());
                btn.set_active(true);
            }

            let category = option.category.clone();
            let st = Rc::clone(&state);
            btn.connect_toggled(move |b| {
                if !st.is_syncing() && b.is_active() {
                    st.apply(i, &category);
                }
            });

            toggle_buttons.push(btn.clone());
            wide_box.append(&btn);
        }

        // --- Narrow mode: CheckButtons in a popover ---
        let popover = gtk::Popover::new();
        let popover_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        popover_box.set_margin_top(12);
        popover_box.set_margin_bottom(12);
        popover_box.set_margin_start(12);
        popover_box.set_margin_end(12);

        let mut first_radio: Option<gtk::CheckButton> = None;
        let mut radio_buttons: Vec<gtk::CheckButton> = Vec::with_capacity(options.len());

        for (i, option) in options.iter().enumerate() {
            let radio = gtk::CheckButton::with_label(&option.label);
            if let Some(ref group) = first_radio {
                radio.set_group(Some(group));
            } else {
                first_radio = Some(radio.clone());
                radio.set_active(true);
            }

            let category = option.category.clone();
            let st = Rc::clone(&state);
            let popover_weak = popover.downgrade();
            radio.connect_toggled(move |b| {
                if !st.is_syncing() && b.is_active() {
                    st.apply(i, &category);
                    if let Some(p) = popover_weak.upgrade() {
                        p.popdown();
                    }
                }
            });

            radio_buttons.push(radio.clone());
            popover_box.append(&radio);
        }

        popover.set_child(Some(&popover_box));
        imp.menu_button.set_popover(Some(&popover));

        // --- Synchronize active state when breakpoint switches modes ---
        let toggle_buttons = Rc::new(toggle_buttons);
        let radio_buttons = Rc::new(radio_buttons);
        let wide_box_ref = imp.wide_box.clone();

        wide_box_ref.connect_notify_local(Some("visible"), move |wb, _| {
            state.syncing.set(true);
            let idx = state.active_index.get();
            if wb.is_visible() {
                if let Some(btn) = toggle_buttons.get(idx) {
                    btn.set_active(true);
                }
            } else if let Some(radio) = radio_buttons.get(idx) {
                radio.set_active(true);
            }
            state.syncing.set(false);
        });

        widget
    }
}
