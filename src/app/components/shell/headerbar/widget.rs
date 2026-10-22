use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use libadwaita::prelude::BinExt;
use libadwaita::subclass::prelude::BinImpl;

use super::HistoryMenuRow;
use crate::app::components::shell::layout::HEADER_COMPACT_WIDTH_PX;
use crate::app::components::{labels, Drawer};

mod imp {

    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/headerbar.ui")]
    pub struct AppHeaderBar {
        #[template_child]
        pub breakpoint_bin: TemplateChild<libadwaita::BreakpointBin>,

        #[template_child]
        pub compact_breakpoint: TemplateChild<libadwaita::Breakpoint>,

        #[template_child]
        pub main_header: TemplateChild<libadwaita::HeaderBar>,

        #[template_child]
        pub start_drawer: TemplateChild<Drawer>,

        #[template_child]
        pub drawer_buttons: TemplateChild<gtk::Box>,

        #[template_child]
        pub navigation_panel_toggle: TemplateChild<gtk::ToggleButton>,

        #[template_child]
        pub go_back: TemplateChild<gtk::Button>,

        #[template_child]
        pub go_forward: TemplateChild<gtk::Button>,

        #[template_child]
        pub back_right_click: TemplateChild<gtk::GestureClick>,

        #[template_child]
        pub back_long_press: TemplateChild<gtk::GestureLongPress>,

        #[template_child]
        pub forward_right_click: TemplateChild<gtk::GestureClick>,

        #[template_child]
        pub forward_long_press: TemplateChild<gtk::GestureLongPress>,

        #[template_child]
        pub back_history: TemplateChild<gtk::Popover>,

        #[template_child]
        pub back_history_list: TemplateChild<gtk::ListBox>,

        #[template_child]
        pub forward_history: TemplateChild<gtk::Popover>,

        #[template_child]
        pub forward_history_list: TemplateChild<gtk::ListBox>,

        #[template_child]
        pub title_stack: TemplateChild<gtk::Stack>,

        #[template_child]
        pub window_title: TemplateChild<libadwaita::WindowTitle>,

        #[template_child]
        pub custom_title: TemplateChild<libadwaita::Bin>,

        #[template_child]
        pub page_buttons: TemplateChild<gtk::Box>,

        pub pages: RefCell<HashMap<String, PageState>>,

        pub page_handlers: RefCell<Vec<(gtk::Widget, glib::SignalHandlerId)>>,

        pub leaving: RefCell<Vec<(gtk::Widget, glib::SourceId)>>,

        pub title_leaving: RefCell<Option<glib::SourceId>>,

        pub active: RefCell<Option<String>>,

        pub narrow: Cell<bool>,
        pub compact_allowed: Cell<bool>,
        pub compact: Cell<bool>,
        pub started: Cell<bool>,
        pub wide_width: Cell<i32>,

        #[template_child]
        pub selection_header: TemplateChild<libadwaita::HeaderBar>,

        #[template_child]
        pub selection_title: TemplateChild<libadwaita::WindowTitle>,

        #[template_child]
        pub select_all: TemplateChild<gtk::Button>,

        #[template_child]
        pub cancel: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AppHeaderBar {
        const NAME: &'static str = "AppHeaderBar";
        type Type = super::AppHeaderBar;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AppHeaderBar {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_layout_manager(None::<gtk::LayoutManager>);
            self.compact_allowed.set(true);
            self.back_history.set_parent(&*self.go_back);
            self.forward_history.set_parent(&*self.go_forward);

            self.compact_breakpoint.set_condition(Some(
                &libadwaita::BreakpointCondition::new_length(
                    libadwaita::BreakpointConditionLengthType::MaxWidth,
                    HEADER_COMPACT_WIDTH_PX,
                    libadwaita::LengthUnit::Px,
                ),
            ));

            let obj = self.obj();
            self.compact_breakpoint.connect_apply(clone!(
                #[weak]
                obj,
                move |_| obj.set_narrow(true)
            ));
            self.compact_breakpoint.connect_unapply(clone!(
                #[weak]
                obj,
                move |_| obj.set_narrow(false)
            ));
            obj.connect_map(|header| {
                if header.imp().started.get() {
                    return;
                }
                glib::timeout_add_local_once(
                    STARTUP_DELAY,
                    clone!(
                        #[weak]
                        header,
                        move || header.imp().started.set(true)
                    ),
                );
            });
            self.start_drawer.set_open_check(clone!(
                #[weak]
                obj,
                #[upgrade_or]
                false,
                move |drawer| obj.min_width() + drawer.content_width() <= obj.width()
            ));
        }

        fn dispose(&self) {
            self.back_history.unparent();
            self.forward_history.unparent();
        }
    }
    impl WidgetImpl for AppHeaderBar {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let (min, natural, min_baseline, natural_baseline) =
                self.breakpoint_bin.measure(orientation, for_size);
            match orientation {
                gtk::Orientation::Horizontal => (min, natural, min_baseline, natural_baseline),
                _ => (natural, natural, natural_baseline, natural_baseline),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let obj = self.obj();
            if self.compact.get() && self.start_drawer.is_showing() && obj.min_width() > width {
                obj.collapse_now();
            }
            self.breakpoint_bin.allocate(width, height, baseline, None);
        }
    }
    impl BinImpl for AppHeaderBar {}
}

/// Until then, the header switches without animating
const STARTUP_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

const ADDED_CLASS: &str = "header-button--added";
const REMOVED_CLASS: &str = "header-button--removed";
const ANIMATION_DURATION: std::time::Duration = std::time::Duration::from_millis(250);

fn animate_in(widget: &gtk::Widget) {
    widget.add_css_class(ADDED_CLASS);
    glib::timeout_add_local_once(
        ANIMATION_DURATION,
        clone!(
            #[weak]
            widget,
            move || widget.remove_css_class(ADDED_CLASS)
        ),
    );
}

#[derive(Debug)]
pub struct PageState {
    pub(super) title: String,
    pub(super) subtitle: String,
    pub(super) title_revealed: bool,
    pub(super) title_widget: Option<gtk::Widget>,
    pub(super) end: Vec<gtk::Widget>,
}

impl Default for PageState {
    fn default() -> Self {
        Self {
            title: String::new(),
            subtitle: String::new(),
            title_revealed: true,
            title_widget: None,
            end: vec![],
        }
    }
}

pub struct HistoryMenuItem {
    pub id: u64,
    pub icon: &'static str,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistorySide {
    Back,
    Forward,
}

glib::wrapper! {
    pub struct AppHeaderBar(ObjectSubclass<imp::AppHeaderBar>) @extends gtk::Widget, libadwaita::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for AppHeaderBar {
    fn default() -> Self {
        Self::new()
    }
}

impl AppHeaderBar {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub(super) fn update_page(&self, name: &str, f: impl FnOnce(&mut PageState)) {
        f(self
            .imp()
            .pages
            .borrow_mut()
            .entry(name.to_string())
            .or_default());
        if self.imp().active.borrow().as_deref() == Some(name) {
            self.render();
        }
    }

    pub(super) fn remove_page(&self, name: &str) {
        self.imp().pages.borrow_mut().remove(name);
        if self.imp().active.borrow().as_deref() == Some(name) {
            self.render();
        }
    }

    pub fn set_active(&self, name: &str) {
        self.finish_title_leaving();
        self.imp().active.replace(Some(name.to_string()));
        self.render();
    }

    fn render(&self) {
        let imp = self.imp();
        let active = imp.active.borrow();
        let pages = imp.pages.borrow();
        let page = active.as_deref().and_then(|name| pages.get(name));

        match page.and_then(|page| page.title_widget.as_ref()) {
            Some(widget) => {
                if imp.custom_title.child().as_ref() != Some(widget) {
                    imp.custom_title.set_child(Some(widget));
                }
                imp.title_stack.set_visible_child(&*imp.custom_title);
            }
            None => {
                imp.custom_title.set_child(None::<&gtk::Widget>);
                let (title, subtitle, revealed) = page
                    .map(|page| {
                        (
                            page.title.as_str(),
                            page.subtitle.as_str(),
                            page.title_revealed,
                        )
                    })
                    .unwrap_or_default();
                imp.window_title.set_title(title);
                imp.window_title.set_subtitle(subtitle);
                let leaving = imp.title_leaving.borrow().is_some();
                imp.window_title
                    .set_opacity(if revealed || leaving { 1.0 } else { 0.0 });
                imp.title_stack.set_visible_child(&*imp.window_title);
            }
        }

        let end = page.map(|page| page.end.as_slice()).unwrap_or_default();
        let unchanged = imp
            .page_handlers
            .borrow()
            .iter()
            .map(|(widget, _)| widget)
            .eq(end);
        if unchanged {
            self.sync_optional_buttons();
            return;
        }
        self.finish_buttons_leaving();
        for (widget, handler) in imp.page_handlers.take() {
            widget.disconnect(handler);
            imp.page_buttons.remove(&widget);
        }
        for widget in end {
            imp.page_buttons.append(widget);
            let handler = widget.connect_visible_notify(clone!(
                #[weak(rename_to = header)]
                self,
                move |_| header.sync_optional_buttons()
            ));
            imp.page_handlers
                .borrow_mut()
                .push((widget.clone(), handler));
        }
        self.sync_optional_buttons();
    }

    pub fn wide_width(&self) -> i32 {
        let imp = self.imp();
        if !imp.compact.get() {
            imp.wide_width.set(self.min_width());
        }
        imp.wide_width.get()
    }

    fn set_narrow(&self, narrow: bool) {
        self.imp().narrow.set(narrow);
        self.update_compact();
    }

    pub fn set_compact_allowed(&self, allowed: bool) {
        self.imp().compact_allowed.set(allowed);
        self.update_compact();
    }

    fn update_compact(&self) {
        let imp = self.imp();
        let compact = imp.narrow.get() && imp.compact_allowed.get();
        if imp.compact.replace(compact) == compact {
            return;
        }
        imp.main_header
            .set_decoration_layout(compact.then_some(":close"));
        self.place_page_buttons(compact);
        self.sync_optional_buttons();

        if !imp.started.get() || (compact && self.min_width() > self.width()) {
            imp.start_drawer.snap_revealed(!compact);
            return;
        }

        let frames = std::cell::Cell::new(0);
        self.add_tick_callback(move |header, _| {
            frames.set(frames.get() + 1);
            if frames.get() < 2 {
                return glib::ControlFlow::Continue;
            }
            let imp = header.imp();
            imp.start_drawer.set_revealed(!imp.compact.get());
            glib::ControlFlow::Break
        });
    }

    fn place_page_buttons(&self, compact: bool) {
        let imp = self.imp();
        let buttons = imp.page_buttons.get();
        if compact {
            imp.main_header.remove(&buttons);
            imp.drawer_buttons.append(&buttons);
        } else {
            imp.drawer_buttons.remove(&buttons);
            imp.main_header.pack_end(&buttons);
        }
    }

    fn collapse_now(&self) {
        self.imp().start_drawer.snap_revealed(false);
    }

    fn min_width(&self) -> i32 {
        self.imp()
            .main_header
            .measure(gtk::Orientation::Horizontal, -1)
            .0
    }

    fn sync_optional_buttons(&self) {
        let imp = self.imp();
        let any = imp
            .page_handlers
            .borrow()
            .iter()
            .any(|(widget, _)| widget.get_visible())
            || !imp.leaving.borrow().is_empty();
        imp.page_buttons.set_visible(any);
        imp.start_drawer.set_collapsible(imp.compact.get());
    }

    pub(super) fn animate_added(&self, widget: &gtk::Widget) {
        let imp = self.imp();
        if widget.parent().as_ref() != Some(imp.page_buttons.upcast_ref()) {
            return;
        }
        if imp.start_drawer.is_collapsed() {
            animate_in(imp.start_drawer.handle().upcast_ref());
        } else {
            animate_in(widget);
        }
    }

    fn title_showing(&self, name: &str) -> bool {
        let imp = self.imp();
        imp.active.borrow().as_deref() == Some(name)
            && imp.title_stack.visible_child().as_ref() == Some(imp.window_title.upcast_ref())
    }

    pub(super) fn animate_title_shown(&self, name: &str) {
        self.finish_title_leaving();
        if self.title_showing(name) {
            animate_in(self.imp().window_title.upcast_ref());
        }
    }

    pub(super) fn animate_title_hidden(&self, name: &str) {
        let imp = self.imp();
        if !self.title_showing(name) || imp.title_leaving.borrow().is_some() {
            return;
        }
        imp.window_title.set_opacity(1.0);
        imp.window_title.add_css_class(REMOVED_CLASS);
        let source = glib::timeout_add_local_once(
            ANIMATION_DURATION,
            clone!(
                #[weak(rename_to = header)]
                self,
                move || {
                    let imp = header.imp();
                    imp.title_leaving.take();
                    imp.window_title.remove_css_class(REMOVED_CLASS);
                    header.render();
                }
            ),
        );
        imp.title_leaving.replace(Some(source));
    }

    fn finish_title_leaving(&self) {
        let imp = self.imp();
        if let Some(source) = imp.title_leaving.take() {
            source.remove();
            imp.window_title.remove_css_class(REMOVED_CLASS);
        }
    }

    pub(super) fn detach_end(&self, widget: &gtk::Widget) {
        let imp = self.imp();
        let detached = {
            let mut handlers = imp.page_handlers.borrow_mut();
            let index = handlers.iter().position(|(w, _)| w == widget);
            index.map(|index| handlers.remove(index))
        };
        if let Some((widget, handler)) = detached {
            widget.disconnect(handler);
            if !imp.start_drawer.is_collapsed() && widget.is_drawable() {
                let picture = gtk::WidgetPaintable::new(Some(&widget)).current_image();
                let picture = gtk::Picture::for_paintable(&picture);
                picture.set_can_target(false);
                imp.page_buttons
                    .insert_child_after(&picture, widget.prev_sibling().as_ref());
                self.animate_removed(picture.upcast());
            }
            imp.page_buttons.remove(&widget);
        }
        self.sync_optional_buttons();
    }

    fn animate_removed(&self, picture: gtk::Widget) {
        picture.add_css_class(REMOVED_CLASS);
        let source = glib::timeout_add_local_once(
            ANIMATION_DURATION,
            clone!(
                #[weak(rename_to = header)]
                self,
                #[strong]
                picture,
                move || {
                    let imp = header.imp();
                    imp.leaving.borrow_mut().retain(|(w, _)| w != &picture);
                    imp.page_buttons.remove(&picture);
                    header.sync_optional_buttons();
                }
            ),
        );
        self.imp().leaving.borrow_mut().push((picture, source));
    }

    fn finish_buttons_leaving(&self) {
        let imp = self.imp();
        for (picture, source) in imp.leaving.take() {
            source.remove();
            imp.page_buttons.remove(&picture);
        }
    }

    // --- Fixed control signals ---

    pub fn connect_go_back<F: Fn() + 'static>(&self, f: F) {
        self.imp().go_back.connect_clicked(move |_| f());
    }

    pub fn connect_go_forward<F: Fn() + 'static>(&self, f: F) {
        self.imp().go_forward.connect_clicked(move |_| f());
    }

    pub fn connect_history_menu<F: Fn(HistorySide) + 'static>(&self, f: F) {
        let imp = self.imp();
        let f = std::rc::Rc::new(f);
        for (right_click, long_press, side) in [
            (
                &*imp.back_right_click,
                &*imp.back_long_press,
                HistorySide::Back,
            ),
            (
                &*imp.forward_right_click,
                &*imp.forward_long_press,
                HistorySide::Forward,
            ),
        ] {
            let on_click = f.clone();
            right_click.connect_pressed(move |gesture, _, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                on_click(side);
            });
            let on_press = f.clone();
            long_press.connect_pressed(move |gesture, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                on_press(side);
            });
        }
    }

    pub fn connect_history_pick<F: Fn(u64) + 'static>(&self, f: F) {
        let imp = self.imp();
        let f = std::rc::Rc::new(f);
        for (popover, list) in [
            (&*imp.back_history, &*imp.back_history_list),
            (&*imp.forward_history, &*imp.forward_history_list),
        ] {
            let on_pick = f.clone();
            list.connect_row_activated(clone!(
                #[weak]
                popover,
                move |_, row| {
                    popover.popdown();
                    if let Some(row) = row.downcast_ref::<HistoryMenuRow>() {
                        on_pick(row.entry_id());
                    }
                }
            ));
        }
    }

    pub fn show_history_menu(&self, side: HistorySide, items: Vec<HistoryMenuItem>) {
        let imp = self.imp();
        let (popover, list) = match side {
            HistorySide::Back => (&imp.back_history, &imp.back_history_list),
            HistorySide::Forward => (&imp.forward_history, &imp.forward_history_list),
        };
        list.remove_all();
        for item in &items {
            list.append(&HistoryMenuRow::new(item));
        }
        popover.popup();
    }

    pub fn connect_select_all<F: Fn() + 'static>(&self, f: F) {
        self.imp().select_all.connect_clicked(move |_| f());
    }

    pub fn connect_selection_cancel<F: Fn() + 'static>(&self, f: F) {
        self.imp().cancel.connect_clicked(move |_| f());
    }

    // --- Fixed control state ---

    pub fn set_utility_panel_beside(&self, open_beside: bool) {
        self.imp()
            .main_header
            .set_show_end_title_buttons(!open_beside);
    }

    pub fn set_can_go_back(&self, can_go_back: bool) {
        self.imp().go_back.set_sensitive(can_go_back);
    }

    pub fn set_can_go_forward(&self, can_go_forward: bool) {
        self.imp().go_forward.set_sensitive(can_go_forward);
    }

    pub fn set_select_all_possible(&self, possible: bool) {
        self.imp().select_all.set_visible(possible);
    }

    pub fn is_selection_active(&self) -> bool {
        self.imp().selection_header.is_visible()
    }

    pub fn set_selection_active(&self, active: bool) {
        let imp = self.imp();
        if active {
            imp.selection_title
                .set_title(&labels::n_tracks_selected_label(0));
            imp.selection_title.set_visible(true);
            imp.selection_header.set_visible(true);
        } else {
            imp.selection_title.set_visible(false);
            imp.selection_header.set_visible(false);
        }
    }

    pub fn set_all_selected(&self, all_selected: bool) {
        self.imp().select_all.set_label(&if all_selected {
            // translators: Button label. Deselects all songs, once they are all selected.
            gettext("Deselect All")
        } else {
            // translators: Button label. Selects all visible songs.
            gettext("Select All")
        });
    }

    pub fn set_selection_count(&self, count: usize) {
        self.imp()
            .selection_title
            .set_title(&labels::n_tracks_selected_label(count));
    }
}
