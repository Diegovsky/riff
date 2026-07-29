use crate::app::components::display_add_css_provider;
use crate::app::loader::ImageLoader;
use crate::app::models::SongModel;
use crate::app::Worker;
use gdk::Rectangle;
use gio::MenuModel;
use glib::subclass::InitializingObject;

use gtk::graphene::Point;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

mod imp {

    use super::*;

    const SONG_CLASS: &str = "song--playing";
    const LIKED_CLASS: &str = "song--liked";

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/song.ui")]
    pub struct SongWidget {
        #[template_child]
        pub song_index: TemplateChild<gtk::Label>,

        #[template_child]
        pub song_icon: TemplateChild<gtk::Spinner>,

        #[template_child]
        pub song_checkbox: TemplateChild<gtk::CheckButton>,

        #[template_child]
        pub song_title: TemplateChild<gtk::Label>,

        #[template_child]
        pub song_artist: TemplateChild<gtk::Label>,

        #[template_child]
        pub song_length: TemplateChild<gtk::Label>,

        #[template_child]
        pub like_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub menu_btn: TemplateChild<gtk::MenuButton>,

        #[template_child]
        pub song_cover: TemplateChild<gtk::Image>,

        pub like_handler_id: std::cell::RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SongWidget {
        const NAME: &'static str = "SongWidget";
        type Type = super::SongWidget;
        type ParentType = gtk::Grid;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    lazy_static! {
        static ref PROPERTIES: [glib::ParamSpec; 3] = [
            glib::ParamSpecBoolean::builder("playing").build(),
            glib::ParamSpecBoolean::builder("selected").build(),
            glib::ParamSpecBoolean::builder("liked").build()
        ];
    }

    impl ObjectImpl for SongWidget {
        fn properties() -> &'static [glib::ParamSpec] {
            &*PROPERTIES
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "playing" => {
                    let is_playing = value
                        .get()
                        .expect("type conformity checked by `Object::set_property`");
                    if is_playing {
                        self.obj().add_css_class(SONG_CLASS);
                    } else {
                        self.obj().remove_css_class(SONG_CLASS);
                    }
                }
                "selected" => {
                    let is_selected = value
                        .get()
                        .expect("type conformity checked by `Object::set_property`");
                    self.song_checkbox.set_active(is_selected);
                }
                "liked" => {
                    let is_liked: bool = value
                        .get()
                        .expect("type conformity checked by `Object::set_property`");
                    if is_liked {
                        self.obj().add_css_class(LIKED_CLASS);
                        self.like_btn.set_icon_name("starred-symbolic");
                        self.like_btn.set_tooltip_text(Some("Unlike"));
                    } else {
                        self.obj().remove_css_class(LIKED_CLASS);
                        self.like_btn.set_icon_name("non-starred-symbolic");
                        self.like_btn.set_tooltip_text(Some("Like"));
                    }
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "playing" => self.obj().has_css_class(SONG_CLASS).to_value(),
                "selected" => self.song_checkbox.is_active().to_value(),
                "liked" => self.obj().has_css_class(LIKED_CLASS).to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.song_checkbox.set_sensitive(false);
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for SongWidget {}
    impl GridImpl for SongWidget {}
}

glib::wrapper! {
    pub struct SongWidget(ObjectSubclass<imp::SongWidget>) @extends gtk::Widget, gtk::Grid;
}

impl Default for SongWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl SongWidget {
    pub fn new() -> Self {
        display_add_css_provider(resource!("/components/song.css"));
        glib::Object::new()
    }

    pub fn set_actions(&self, actions: Option<&gio::ActionGroup>) {
        self.insert_action_group("song", actions);
    }

    pub fn set_menu(&self, menu: Option<&MenuModel>) {
        if menu.is_some() {
            let widget = self.imp();
            widget.menu_btn.set_menu_model(menu);
            widget.menu_btn.add_css_class("song__menu--enabled");
            // Resets the pointing_to target when closing the popup.
            // This prevents it from being messes up if it was opened by right click.
            widget
                .menu_btn
                .popover()
                .unwrap()
                .connect_closed(|popover| {
                    popover.set_pointing_to(None);
                });
        }
    }

    /// Shows the menu popup anchored to the given coordinates.
    ///
    /// Note: coordinates are assumed to be relative to the widget.
    pub fn show_menu(&self, x: f64, y: f64) {
        let widget = self.imp();
        let root = self.root().unwrap();

        let point = self
            .compute_point(&widget.menu_btn.get(), &Point::zero())
            .unwrap();
        let popover = widget.menu_btn.popover().unwrap();
        popover.set_pointing_to(Some(&Rectangle::new(
            (point.x() as f64 + x) as _,
            (point.y() as f64 + y) as _,
            1,
            1,
        )));
        popover.popup();
    }

    pub fn connect_like<F: Fn() + 'static>(&self, f: F) {
        let widget = self.imp();
        // Disconnect previous handler to avoid stacking handlers on widget reuse
        if let Some(old_id) = widget.like_handler_id.borrow_mut().take() {
            widget.like_btn.disconnect(old_id);
        }
        let handler_id = widget.like_btn.connect_clicked(move |_| {
            f();
        });
        widget.like_handler_id.replace(Some(handler_id));
    }

    pub fn disconnect_like(&self) {
        let widget = self.imp();
        if let Some(handler_id) = widget.like_handler_id.borrow_mut().take() {
            widget.like_btn.disconnect(handler_id);
        }
    }

    fn set_show_cover(&self, show_cover: bool) {
        let song_class = "song--cover";
        if show_cover {
            self.add_css_class(song_class);
        } else {
            self.remove_css_class(song_class);
        }
    }

    fn set_image(&self, pixbuf: &gdk_pixbuf::Pixbuf) {
        let texture = gdk::Texture::for_pixbuf(pixbuf);
        self.imp().song_cover.set_paintable(Some(&texture));
    }

    pub fn set_art(&self, model: &SongModel, worker: Worker) {
        if let Some(url) = model
            .description()
            .art
            .as_ref()
            .and_then(|s| s.best_for_width(48))
            .map(str::to_owned)
        {
            let _self = self.downgrade();
            worker.send_local_task(async move {
                if let Some(_self) = _self.upgrade() {
                    let loader = ImageLoader::new();
                    let result = loader.load_remote(&url, "jpg", 100, 100).await;
                    if let Some(pixbuf) = result.as_ref() {
                        _self.set_image(pixbuf);
                    }
                }
            });
        }
    }

    pub fn bind(&self, model: &SongModel, worker: Worker, show_cover: bool) {
        let widget = self.imp();

        model.bind_title(&*widget.song_title, "label");
        model.bind_artist(&*widget.song_artist, "label");
        model.bind_duration(&*widget.song_length, "label");
        model.bind_playing(self, "playing");
        model.bind_selected(self, "selected");
        model.bind_liked(self, "liked");

        self.set_show_cover(show_cover);
        if show_cover {
            self.set_art(model, worker);
        } else {
            model.bind_index(&*widget.song_index, "label");
        }
    }
}
