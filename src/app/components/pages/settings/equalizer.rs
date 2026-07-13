use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use super::lock_button;

const SETTINGS: &str = "dev.diegovsky.Riff";

/// GSettings keys for the ten EQ bands, in ascending frequency order.
const EQ_BAND_KEYS: [&str; 10] = [
    "eq-band-0",
    "eq-band-1",
    "eq-band-2",
    "eq-band-3",
    "eq-band-4",
    "eq-band-5",
    "eq-band-6",
    "eq-band-7",
    "eq-band-8",
    "eq-band-9",
];

mod imp {
    use super::*;
    use libadwaita::subclass::prelude::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/equalizer.ui")]
    pub struct EqualizerWidget {
        #[template_child]
        pub eq_lock: TemplateChild<gtk::ToggleButton>,

        #[template_child]
        pub eq_bands_box: TemplateChild<gtk::Box>,

        #[template_child]
        pub eq_band_0: TemplateChild<gtk::Scale>,

        #[template_child]
        pub eq_band_1: TemplateChild<gtk::Scale>,

        #[template_child]
        pub eq_band_2: TemplateChild<gtk::Scale>,

        #[template_child]
        pub eq_band_3: TemplateChild<gtk::Scale>,

        #[template_child]
        pub eq_band_4: TemplateChild<gtk::Scale>,

        #[template_child]
        pub eq_band_5: TemplateChild<gtk::Scale>,

        #[template_child]
        pub eq_band_6: TemplateChild<gtk::Scale>,

        #[template_child]
        pub eq_band_7: TemplateChild<gtk::Scale>,

        #[template_child]
        pub eq_band_8: TemplateChild<gtk::Scale>,

        #[template_child]
        pub eq_band_9: TemplateChild<gtk::Scale>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for EqualizerWidget {
        const NAME: &'static str = "EqualizerWidget";
        type Type = super::EqualizerWidget;
        type ParentType = libadwaita::PreferencesGroup;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for EqualizerWidget {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().bind_settings();
            self.obj().connect_lock();
        }
    }

    impl WidgetImpl for EqualizerWidget {}
    impl PreferencesGroupImpl for EqualizerWidget {}
}

glib::wrapper! {
    pub struct EqualizerWidget(ObjectSubclass<imp::EqualizerWidget>)
        @extends gtk::Widget, libadwaita::PreferencesGroup;
}

impl Default for EqualizerWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl EqualizerWidget {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Returns the ten band scales in ascending frequency order.
    fn band_scales(&self) -> [gtk::Scale; 10] {
        let w = self.imp();
        [
            w.eq_band_0.get(),
            w.eq_band_1.get(),
            w.eq_band_2.get(),
            w.eq_band_3.get(),
            w.eq_band_4.get(),
            w.eq_band_5.get(),
            w.eq_band_6.get(),
            w.eq_band_7.get(),
            w.eq_band_8.get(),
            w.eq_band_9.get(),
        ]
    }

    fn bind_settings(&self) {
        let settings = gio::Settings::new(SETTINGS);

        // Bind each band's adjustment one-way to avoid the bidirectional
        // feedback loop that reverts single scale steps.
        for (key, scale) in EQ_BAND_KEYS.iter().zip(self.band_scales()) {
            let adjustment = scale.adjustment();
            adjustment.set_value(settings.double(key));
            let settings = settings.clone();
            let key = key.to_string();
            adjustment.connect_value_changed(move |adj| {
                let _ = settings.set_double(&key, adj.value());
            });
        }
    }

    /// Wire up the lock button: while locked, the band sliders are not editable,
    /// and the button's icon reflects the lock state.
    fn connect_lock(&self) {
        lock_button::setup_lock(&self.imp().eq_lock, &self.imp().eq_bands_box.get());
    }

    /// Lock the equalizer bands (UI-only; not persisted). Called on creation and
    /// each time the preferences page is opened so the lock is never stateful.
    pub fn lock(&self) {
        self.imp().eq_lock.set_active(true);
    }
}
