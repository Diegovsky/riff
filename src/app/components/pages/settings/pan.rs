use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use super::lock_button;

const SETTINGS: &str = "dev.diegovsky.Riff";

mod imp {
    use super::*;
    use libadwaita::subclass::prelude::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/pan.ui")]
    pub struct PanWidget {
        #[template_child]
        pub pan_lock: TemplateChild<gtk::ToggleButton>,

        #[template_child]
        pub pan_balance: TemplateChild<gtk::Scale>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PanWidget {
        const NAME: &'static str = "PanWidget";
        type Type = super::PanWidget;
        type ParentType = libadwaita::PreferencesGroup;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PanWidget {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().bind_settings();
            self.obj().connect_lock();
        }
    }

    impl WidgetImpl for PanWidget {}
    impl PreferencesGroupImpl for PanWidget {}
}

glib::wrapper! {
    pub struct PanWidget(ObjectSubclass<imp::PanWidget>)
        @extends gtk::Widget, libadwaita::PreferencesGroup;
}

impl Default for PanWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl PanWidget {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn bind_settings(&self) {
        let settings = gio::Settings::new(SETTINGS);
        let imp = self.imp();

        // Bind the balance one-way to avoid the bidirectional feedback loop that
        // reverts single scale steps (same approach as the equalizer bands).
        let scale = imp.pan_balance.get();
        let adjustment = scale.adjustment();
        adjustment.set_value(settings.double("pan"));
        let settings_clone = settings.clone();
        adjustment.connect_value_changed(move |adj| {
            let _ = settings_clone.set_double("pan", adj.value());
        });
    }

    /// Wire up the lock button: while locked, the balance slider is not editable,
    /// and the button's icon reflects the lock state.
    fn connect_lock(&self) {
        lock_button::setup_lock(&self.imp().pan_lock, &self.imp().pan_balance.get());
    }

    /// Lock the balance slider (UI-only; not persisted). Called on creation and
    /// each time the preferences page is opened so the lock is never stateful.
    pub fn lock(&self) {
        self.imp().pan_lock.set_active(true);
    }
}
