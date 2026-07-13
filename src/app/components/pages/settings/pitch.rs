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
    #[template(resource = "/dev/diegovsky/Riff/components/pitch.ui")]
    pub struct PitchWidget {
        #[template_child]
        pub pitch_lock: TemplateChild<gtk::ToggleButton>,

        #[template_child]
        pub pitch_cents: TemplateChild<gtk::Scale>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PitchWidget {
        const NAME: &'static str = "PitchWidget";
        type Type = super::PitchWidget;
        type ParentType = libadwaita::PreferencesGroup;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PitchWidget {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().bind_settings();
            self.obj().connect_lock();
        }
    }

    impl WidgetImpl for PitchWidget {}
    impl PreferencesGroupImpl for PitchWidget {}
}

glib::wrapper! {
    pub struct PitchWidget(ObjectSubclass<imp::PitchWidget>)
        @extends gtk::Widget, libadwaita::PreferencesGroup;
}

impl Default for PitchWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl PitchWidget {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn bind_settings(&self) {
        let settings = gio::Settings::new(SETTINGS);
        let imp = self.imp();

        // Bind the cents value one-way to avoid the bidirectional feedback loop
        // that reverts single scale steps (same approach as the equalizer bands).
        let scale = imp.pitch_cents.get();
        let adjustment = scale.adjustment();
        adjustment.set_value(settings.double("pitch-cents"));
        let settings_clone = settings.clone();
        adjustment.connect_value_changed(move |adj| {
            let _ = settings_clone.set_double("pitch-cents", adj.value());
        });
    }

    /// Wire up the lock button: while locked, the pitch slider is not editable,
    /// and the button's icon reflects the lock state.
    fn connect_lock(&self) {
        lock_button::setup_lock(&self.imp().pitch_lock, &self.imp().pitch_cents.get());
    }

    /// Lock the pitch slider (UI-only; not persisted). Called on creation and
    /// each time the preferences page is opened so the lock is never stateful.
    pub fn lock(&self) {
        self.imp().pitch_lock.set_active(true);
    }
}
