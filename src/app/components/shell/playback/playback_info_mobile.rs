use gettextrs::gettext;
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
        let label = &self.imp().now_playing_label;
        label.set_markup(&markup);
        // Cap label width so a long title/artist cannot starve the side
        // spacers and walk the bar off-centre (same role as ellipsis inside
        // the desktop `now_playing_start` column).
        label.set_max_width_chars(48);
    }

    pub fn reset_info(&self) {
        let label = &self.imp().now_playing_label;
        label.set_text(&gettext("No Track Playing"));
        label.set_max_width_chars(-1);
    }
}
