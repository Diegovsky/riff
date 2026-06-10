use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

mod imp {

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/playback_info.ui")]
    pub struct PlaybackInfoWidget {
        #[template_child]
        pub playing_image: TemplateChild<gtk::Picture>,

        #[template_child]
        pub song_info_box: TemplateChild<gtk::Box>,

        #[template_child]
        pub song_title: TemplateChild<gtk::Label>,

        #[template_child]
        pub song_artist: TemplateChild<gtk::Label>,


    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlaybackInfoWidget {
        const NAME: &'static str = "PlaybackInfoWidget";
        type Type = super::PlaybackInfoWidget;
        type ParentType = gtk::Button;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PlaybackInfoWidget {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_hover_animations();
        }
    }
    impl WidgetImpl for PlaybackInfoWidget {}
    impl ButtonImpl for PlaybackInfoWidget {}
}

glib::wrapper! {
    pub struct PlaybackInfoWidget(ObjectSubclass<imp::PlaybackInfoWidget>) @extends gtk::Widget, gtk::Button;
}

impl PlaybackInfoWidget {
    fn setup_hover_animations(&self) {
    }

    pub fn set_title_and_artist(&self, title: &str, artist: &str) {
        let widget = self.imp();
        widget.song_title.set_text(title);
        widget.song_artist.set_text(artist);
        widget.song_info_box.set_visible(true);
    }

    pub fn reset_info(&self) {
        let widget = self.imp();
        widget
            .song_title
            // translators: Short text displayed instead of a song title when nothing plays
            .set_text(&gettext("No song playing"));
        widget.song_artist.set_text("");
        widget.song_info_box.set_visible(false);
        widget
            .playing_image
            .set_paintable(None::<gdk::Paintable>.as_ref());
    }

    pub fn set_info_visible(&self, visible: bool) {
        self.imp().song_info_box.set_visible(visible);
    }

    pub fn set_artwork(&self, pixbuf: &gdk_pixbuf::Pixbuf) {
        let texture = gdk::Texture::for_pixbuf(pixbuf);
        self.imp().playing_image.set_paintable(Some(&texture));
    }

}
