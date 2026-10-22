use std::cell::{Cell, RefCell};
use std::time::Duration;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use libadwaita::subclass::prelude::*;

use crate::app::components::display_add_css_provider;

const COLLAPSE_DELAY: Duration = Duration::from_millis(300);

type OpenCheck = Box<dyn Fn(&Drawer) -> bool>;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate, glib::Properties)]
    #[template(resource = "/dev/diegovsky/Riff/components/drawer.ui")]
    #[properties(wrapper_type = super::Drawer)]
    pub struct Drawer {
        #[property(get, set = Self::set_content)]
        pub content: RefCell<Option<gtk::Widget>>,
        #[template_child]
        pub motion: TemplateChild<gtk::EventControllerMotion>,
        #[template_child]
        pub revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub handle: TemplateChild<gtk::Button>,
        pub collapsible: Cell<bool>,
        pub open_check: RefCell<Option<OpenCheck>>,
        pub close_source: RefCell<Option<glib::SourceId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Drawer {
        const NAME: &'static str = "Drawer";
        type Type = super::Drawer;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Drawer {
        fn constructed(&self) {
            self.parent_constructed();
            display_add_css_provider(resource!("/components/drawer.css"));
            let obj = self.obj();

            self.motion.connect_enter(clone!(
                #[weak]
                obj,
                move |_, _, _| {
                    obj.cancel_close();
                    obj.open();
                }
            ));
            self.handle.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| obj.open()
            ));
            self.motion.connect_leave(clone!(
                #[weak]
                obj,
                move |_| obj.close_when_left()
            ));
        }
    }

    impl WidgetImpl for Drawer {}
    impl BinImpl for Drawer {}

    impl Drawer {
        fn set_content(&self, content: Option<gtk::Widget>) {
            self.revealer.set_child(content.as_ref());
            self.content.replace(content);
        }
    }
}

glib::wrapper! {
    pub struct Drawer(ObjectSubclass<imp::Drawer>)
        @extends gtk::Widget, libadwaita::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Drawer {
    pub fn handle(&self) -> &gtk::Button {
        &self.imp().handle
    }

    pub fn set_collapsible(&self, collapsible: bool) {
        let imp = self.imp();
        imp.collapsible.set(collapsible);
        imp.handle.set_visible(collapsible);

        let inset = if collapsible {
            imp.handle.measure(gtk::Orientation::Horizontal, -1).0 / 2
        } else {
            0
        };
        imp.revealer.set_margin_start(inset);
        if let Some(content) = self.content() {
            content.set_margin_start(inset);
        }
    }

    pub fn is_collapsed(&self) -> bool {
        self.imp().collapsible.get() && !self.imp().revealer.reveals_child()
    }

    pub fn is_showing(&self) -> bool {
        let revealer = &self.imp().revealer;
        revealer.reveals_child() || revealer.is_child_revealed()
    }

    pub fn set_revealed(&self, revealed: bool) {
        self.imp().revealer.set_reveal_child(revealed);
    }

    pub fn snap_revealed(&self, revealed: bool) {
        let revealer = &self.imp().revealer;
        let duration = revealer.transition_duration();
        revealer.set_transition_duration(0);
        revealer.set_reveal_child(!revealed);
        revealer.set_reveal_child(revealed);
        revealer.set_transition_duration(duration);
    }

    pub fn content_width(&self) -> i32 {
        self.content()
            .map(|content| content.measure(gtk::Orientation::Horizontal, -1).0)
            .unwrap_or(0)
    }

    pub fn set_open_check(&self, check: impl Fn(&Drawer) -> bool + 'static) {
        self.imp().open_check.replace(Some(Box::new(check)));
    }

    fn open(&self) {
        let imp = self.imp();
        if imp.revealer.reveals_child() {
            return;
        }
        let allowed = imp
            .open_check
            .borrow()
            .as_ref()
            .is_none_or(|check| check(self));
        if imp.collapsible.get() && allowed {
            imp.revealer.set_reveal_child(true);
        }
    }

    fn cancel_close(&self) {
        if let Some(source) = self.imp().close_source.take() {
            source.remove();
        }
    }

    fn close_when_left(&self) {
        self.cancel_close();
        let source = glib::timeout_add_local(
            COLLAPSE_DELAY,
            clone!(
                #[weak(rename_to = drawer)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    let imp = drawer.imp();
                    let stay = !imp.collapsible.get() || imp.motion.contains_pointer();
                    if !stay && has_open_popover(imp.revealer.upcast_ref()) {
                        return glib::ControlFlow::Continue;
                    }
                    // Ended by returning Break, so not to be removed again
                    imp.close_source.take();
                    if !stay {
                        imp.revealer.set_reveal_child(false);
                    }
                    glib::ControlFlow::Break
                }
            ),
        );
        self.imp().close_source.replace(Some(source));
    }
}

fn has_open_popover(widget: &gtk::Widget) -> bool {
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if (widget.is::<gtk::Popover>() && widget.is_visible()) || has_open_popover(&widget) {
            return true;
        }
        child = widget.next_sibling();
    }
    false
}

pub fn expose_widgets() {
    Drawer::static_type();
}
