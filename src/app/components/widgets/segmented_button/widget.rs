//! A pill-shaped button that holds several icon buttons in a row and can expand.
//!
//! Expand behavior is controlled by [`ExpandBehavior`]:
//! - [`ExpandBehavior::OnClick`] (default) - pressing the first segment toggles
//!   the control open and closed.
//! - [`ExpandBehavior::OnHover`] - hovering over the container expands it;
//!   leaving collapses it after a short delay. Clicking the first segment only
//!   dispatches its action and does not affect the expanded state.
//! - [`ExpandBehavior::AlwaysExpanded`] - the control is permanently open.
//!   The first segment still dispatches its action on click but cannot collapse
//!   the control.

use std::cell::{Cell, RefCell};

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use crate::app::components::display_add_css_provider;
use crate::app::components::utils::Debouncer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpandBehavior {
    #[default]
    OnClick,
    OnHover,
    AlwaysExpanded,
}

/// Delay before a hover-triggered expansion collapses again after the pointer leaves the widget.
const HOVER_COLLAPSE_DELAY_MS: u32 = 600;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/segmented_button.ui")]
    pub struct SegmentedButtonWidget {
        #[template_child]
        pub revealer: TemplateChild<gtk::Revealer>,

        #[template_child]
        pub extra_box: TemplateChild<gtk::Box>,

        /// Whether the extra segments are currently revealed.
        pub expanded: Cell<bool>,
        /// Number of segments added so far.
        pub count: Cell<usize>,
        /// Current behavior - shared with first-segment click handlers.
        pub behavior: Cell<ExpandBehavior>,
        /// Currently installed hover controller, kept so it can be removed.
        pub hover_controller: RefCell<Option<gtk::EventControllerMotion>>,
        /// Debounces the collapse triggered by leaving the widget in
        /// [`ExpandBehavior::OnHover`].
        pub collapse_debouncer: Debouncer,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SegmentedButtonWidget {
        const NAME: &'static str = "SegmentedButtonWidget";
        type Type = super::SegmentedButtonWidget;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SegmentedButtonWidget {
        fn constructed(&self) {
            self.parent_constructed();
            display_add_css_provider(resource!("/components/segmented_button.css"));
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for SegmentedButtonWidget {}
    impl BoxImpl for SegmentedButtonWidget {}
}

glib::wrapper! {
    pub struct SegmentedButtonWidget(ObjectSubclass<imp::SegmentedButtonWidget>) @extends gtk::Widget, gtk::Box;
}

impl SegmentedButtonWidget {
    fn set_expanded_internal(&self, value: bool) {
        let imp = self.imp();
        if imp.expanded.get() == value {
            return;
        }
        imp.expanded.set(value);
        imp.revealer.set_reveal_child(value);
        if value {
            self.remove_css_class("collapsed");
        } else {
            self.add_css_class("collapsed");
        }
    }
}

pub struct SegmentedButton {
    widget: SegmentedButtonWidget,
}

impl SegmentedButton {
    /// Create a new segmented button with the given expand behavior.
    pub fn new(behavior: ExpandBehavior) -> Self {
        let widget: SegmentedButtonWidget = glib::Object::new();
        let this = Self { widget };
        this.set_expand_behavior(behavior);
        this
    }

    /// Add a new icon segment with its own click action.
    ///
    /// The first call adds the always-visible segment; later calls add segments hidden until the control is expanded.
    pub fn add_icon(
        &self,
        icon_name: &str,
        tooltip: &str,
        on_click: impl Fn() + 'static,
    ) -> gtk::Button {
        let button = gtk::Button::from_icon_name(icon_name);
        button.set_tooltip_text(Some(tooltip));

        let imp = self.widget.imp();
        if imp.count.get() == 0 {
            let widget = self.widget.clone();
            button.connect_clicked(move |_| {
                let behavior = widget.imp().behavior.get();
                if behavior == ExpandBehavior::AlwaysExpanded || behavior == ExpandBehavior::OnHover
                {
                    return;
                }
                let next = !widget.imp().expanded.get();
                widget.set_expanded_internal(next);
            });
            button.connect_clicked(move |_| on_click());
            self.widget.prepend(&button);
        } else {
            button.connect_clicked(move |_| on_click());
            imp.extra_box.append(&button);
        }
        imp.count.set(imp.count.get() + 1);
        button
    }

    /// Switch the expand trigger. Can be called after segments have been added.
    pub fn set_expand_behavior(&self, behavior: ExpandBehavior) {
        let imp = self.widget.imp();
        if let Some(ctrl) = imp.hover_controller.take() {
            self.widget.remove_controller(&ctrl);
        }

        imp.behavior.set(behavior);

        match behavior {
            ExpandBehavior::OnHover => {
                let widget_enter = self.widget.clone();
                let widget_leave = self.widget.clone();

                let ctrl = gtk::EventControllerMotion::new();
                ctrl.connect_enter(move |_, _, _| {
                    widget_enter.imp().collapse_debouncer.stop();
                    widget_enter.set_expanded_internal(true);
                });
                ctrl.connect_leave(move |_| {
                    let widget = widget_leave.clone();
                    widget_leave
                        .imp()
                        .collapse_debouncer
                        .debounce(HOVER_COLLAPSE_DELAY_MS, move || {
                            widget.set_expanded_internal(false)
                        });
                });
                self.widget.add_controller(ctrl.clone());
                imp.hover_controller.replace(Some(ctrl));
            }
            ExpandBehavior::AlwaysExpanded => {
                self.widget.set_expanded_internal(true);
            }
            ExpandBehavior::OnClick => {
                // No controller needed as toggle is wired in add_icon.
            }
        }
    }

    /// Force the control open or closed.
    #[allow(dead_code)]
    pub fn set_expanded(&self, value: bool) {
        if value {
            self.widget.set_expanded_internal(true);
        } else {
            let widget = self.widget.clone();
            glib::idle_add_local_once(move || {
                widget.set_expanded_internal(false);
            });
        }
    }

    /// Whether the extra segments are currently revealed.
    #[allow(dead_code)]
    pub fn is_expanded(&self) -> bool {
        self.widget.imp().expanded.get()
    }

    /// The root GTK widget to insert into the UI.
    pub fn widget(&self) -> &gtk::Widget {
        self.widget.upcast_ref()
    }
}

impl Default for SegmentedButton {
    fn default() -> Self {
        Self::new(ExpandBehavior::default())
    }
}

/// Register the GObject type at app startup.
pub fn expose_widgets() {
    SegmentedButtonWidget::static_type();
}
