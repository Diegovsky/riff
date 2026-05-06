use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

mod imp {

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/playback_info_mobile.ui")]
    pub struct PlaybackInfoMobileWidget {
        #[template_child]
        pub now_playing_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlaybackInfoMobileWidget {
        const NAME: &'static str = "PlaybackInfoMobileWidget";
        type Type = super::PlaybackInfoMobileWidget;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PlaybackInfoMobileWidget {}
    impl WidgetImpl for PlaybackInfoMobileWidget {}
    impl BoxImpl for PlaybackInfoMobileWidget {}
}

glib::wrapper! {
    pub struct PlaybackInfoMobileWidget(ObjectSubclass<imp::PlaybackInfoMobileWidget>) @extends gtk::Widget, gtk::Box;
}

impl PlaybackInfoMobileWidget {
    pub fn set_title_and_artist(&self, title: &str, artist: &str) {
        let markup = format!(
            "<b>{}</b> \u{2014} <small>{}</small>",
            glib::markup_escape_text(title),
            glib::markup_escape_text(artist)
        );
        self.imp().now_playing_label.set_markup(&markup);
    }

    pub fn reset_info(&self) {
        self.imp().now_playing_label.set_text(&gettext("No song playing"));
    }
}
