use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use crate::discord_rpc::rpc::{update_discord_presence, clear_discord_presence};

mod imp {

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/playback_info.ui")]
    pub struct PlaybackInfoWidget {
        #[template_child]
        pub playing_image: TemplateChild<gtk::Image>,

        #[template_child]
        pub current_song_info: TemplateChild<gtk::Label>,
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

    impl ObjectImpl for PlaybackInfoWidget {}
    impl WidgetImpl for PlaybackInfoWidget {}
    impl ButtonImpl for PlaybackInfoWidget {}
}

glib::wrapper! {
    pub struct PlaybackInfoWidget(ObjectSubclass<imp::PlaybackInfoWidget>) @extends gtk::Widget, gtk::Button;
}

// Implements for cover_url OPTIONALLY now (used in DiscordRPC).
impl PlaybackInfoWidget {
    pub fn set_title_and_artist(&self, title: &str, artist: &str, cover_url: Option<&str>) {
        let widget = self.imp();
        
        let title_escaped = glib::markup_escape_text(title);
        let artist_escaped = glib::markup_escape_text(artist);
        let label = format!("<b>{}</b>\n{}", title_escaped.as_str(), artist_escaped.as_str());
        widget.current_song_info.set_label(&label);

        // Discord RPC - Set the RPC.
        println!("[RPC] Setting RPC for song with title '{}' and artist '{}'", title, artist);
        update_discord_presence(title, artist, cover_url);
    }

    pub fn reset_info(&self) {
        let widget = self.imp();
        widget
            .current_song_info
            // translators: Short text displayed instead of a song title when nothing plays
            .set_label(&gettext("No song playing"));
        widget
            .playing_image
            .set_icon_name(Some("emblem-music-symbolic"));
        widget
            .playing_image
            .set_icon_name(Some("emblem-music-symbolic"));

        clear_discord_presence();
    }

    pub fn set_info_visible(&self, visible: bool) {
        self.imp().current_song_info.set_visible(visible);
    }

    pub fn set_artwork(&self, pixbuf: &gdk_pixbuf::Pixbuf) {
        let texture = gdk::Texture::for_pixbuf(pixbuf);
        self.imp().playing_image.set_paintable(Some(&texture));
    }
}
