use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use libadwaita::prelude::BinExt;
use libadwaita::subclass::prelude::BinImpl;

use super::HistoryMenuRow;
use crate::app::components::labels;

mod imp {

    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/headerbar.ui")]
    pub struct AppHeaderBar {
        #[template_child]
        pub main_header: TemplateChild<libadwaita::HeaderBar>,

        #[template_child]
        pub utility_panel_button: TemplateChild<gtk::ToggleButton>,

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
        pub end_box: TemplateChild<gtk::Box>,

        pub pages: RefCell<HashMap<String, PageState>>,

        pub active: RefCell<Option<String>>,

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
            self.back_history.set_parent(&*self.go_back);
            self.forward_history.set_parent(&*self.go_forward);
        }

        fn dispose(&self) {
            self.back_history.unparent();
            self.forward_history.unparent();
        }
    }
    impl WidgetImpl for AppHeaderBar {}
    impl BinImpl for AppHeaderBar {}
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
                imp.window_title
                    .set_opacity(if revealed { 1.0 } else { 0.0 });
                imp.title_stack.set_visible_child(&*imp.window_title);
            }
        }

        while let Some(child) = imp.end_box.first_child() {
            imp.end_box.remove(&child);
        }
        let end = page.map(|page| page.end.as_slice()).unwrap_or_default();
        for widget in end {
            imp.end_box.append(widget);
        }
        imp.end_box.set_visible(!end.is_empty());
    }

    pub(super) fn detach_end(&self, widget: &gtk::Widget) {
        if widget.parent().as_ref() == Some(self.imp().end_box.upcast_ref()) {
            self.imp().end_box.remove(widget);
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

    pub fn set_utility_panel_state(&self, open_beside: bool, side_panel: bool) {
        self.imp()
            .main_header
            .set_show_end_title_buttons(!open_beside);
        self.imp()
            .utility_panel_button
            .set_visible(side_panel && !open_beside);
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
