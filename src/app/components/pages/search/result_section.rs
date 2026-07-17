use glib::object::IsA;
use glib::object::ObjectExt;
use glib::{Object, Properties, SignalHandlerId};
use gtk::prelude::*;
use libadwaita::subclass::prelude::*;
use libadwaita::Bin;

use gtk::subclass::prelude::*;
use gtk::{glib, FlowBox};
use gtk::{CompositeTemplate, FlowBoxChild, Widget};
use std::cell::RefCell;

mod imp {

    use super::*;

    // Object holding the state
    #[derive(Default, Properties, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/result_section.ui")]
    #[properties(wrapper_type = super::ResultSection)]
    pub struct ResultSection {
        #[template_child]
        pub flow_box: TemplateChild<FlowBox>,

        #[property(get, set)]
        label: RefCell<String>,
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for ResultSection {
        const NAME: &'static str = "ResultSection";
        type Type = super::ResultSection;
        type ParentType = Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ResultSection {}

    impl WidgetImpl for ResultSection {}
    impl BinImpl for ResultSection {}
}

glib::wrapper! {
    pub struct ResultSection(ObjectSubclass<imp::ResultSection>)
        @extends Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ResultSection {
    pub fn new() -> Self {
        Object::builder().build()
    }
    #[doc(alias = "gtk_flow_box_bind_model")]
    pub fn bind_model<P: Fn(&glib::Object) -> Widget + 'static>(
        &self,
        model: Option<&impl IsA<gio::ListModel>>,
        create_widget_func: P,
    ) {
        self.imp().flow_box.bind_model(model, create_widget_func);
    }

    pub fn connect_child_activated<F: Fn(&Self, &FlowBoxChild) + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        self.imp().flow_box.connect_child_activated(glib::clone!(
            #[strong(rename_to = this)]
            self,
            move |_, child| f(&this, child)
        ))
    }
}

impl Default for ResultSection {
    fn default() -> Self {
        Self::new()
    }
}
