use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

use crate::app::components::display_add_css_provider;

#[derive(Clone)]
pub struct Saved(Arc<Mutex<Option<oneshot::Sender<()>>>>);

impl Saved {
    pub fn channel() -> (Self, oneshot::Receiver<()>) {
        let (tx, rx) = oneshot::channel();
        (Self(Arc::new(Mutex::new(Some(tx)))), rx)
    }

    pub fn tell(&self) {
        if let Some(tx) = self.0.lock().unwrap().take() {
            let _ = tx.send(());
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlaylistForm {
    pub name: String,
    pub description: Option<String>,
    pub public: bool,
    pub public_changed: bool,
    pub cover: Option<Vec<u8>>,
}

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/playlist_dialog.ui")]
    pub struct PlaylistDialog {
        #[template_child]
        pub details: TemplateChild<gtk::Box>,

        #[template_child]
        pub cover_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub cover_picture: TemplateChild<gtk::Picture>,

        #[template_child]
        pub name_entry: TemplateChild<gtk::Entry>,

        #[template_child]
        pub description_view: TemplateChild<gtk::TextView>,

        #[template_child]
        pub description_placeholder: TemplateChild<gtk::Label>,

        #[template_child]
        pub public_switch: TemplateChild<gtk::Switch>,

        #[template_child]
        pub error_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub save_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub save_stack: TemplateChild<gtk::Stack>,

        #[template_child]
        pub save_label: TemplateChild<gtk::Label>,

        pub cover: RefCell<Option<Vec<u8>>>,

        pub initial_public: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlaylistDialog {
        const NAME: &'static str = "PlaylistDialog";
        type Type = super::PlaylistDialog;
        type ParentType = libadwaita::Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PlaylistDialog {
        fn constructed(&self) {
            self.parent_constructed();
            display_add_css_provider(resource!("/components/playlist_dialog.css"));

            self.name_entry.connect_changed(clone!(
                #[weak(rename_to = imp)]
                self,
                move |entry| {
                    imp.save_button
                        .set_sensitive(!entry.text().trim().is_empty());
                }
            ));

            self.description_view.buffer().connect_changed(clone!(
                #[weak(rename_to = imp)]
                self,
                move |buffer| {
                    imp.description_placeholder
                        .set_visible(buffer.char_count() == 0);
                }
            ));

            self.cover_button.connect_clicked(clone!(
                #[weak(rename_to = imp)]
                self,
                move |_| imp.obj().choose_cover()
            ));
        }
    }

    impl WidgetImpl for PlaylistDialog {}
    impl AdwDialogImpl for PlaylistDialog {}
}

glib::wrapper! {
    pub struct PlaylistDialog(ObjectSubclass<imp::PlaylistDialog>) @extends gtk::Widget, libadwaita::Dialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for PlaylistDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaylistDialog {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn for_editing(name: &str, description: Option<&str>, public: bool) -> Self {
        let dialog = Self::new();
        let imp = dialog.imp();
        // translators: Title of the dialog editing the details of a playlist.
        dialog.set_title(&gettext("Edit Playlist Details"));
        // translators: Button of the dialog editing the details of a playlist: saves them.
        imp.save_label.set_label(&gettext("Save"));
        imp.error_label
            // translators: Shown in the dialog editing the details of a playlist when saving them failed.
            .set_label(&gettext("The playlist could not be saved."));
        imp.name_entry.set_text(name);
        imp.description_view
            .buffer()
            .set_text(description.unwrap_or_default());
        imp.public_switch.set_active(public);
        imp.initial_public.set(public);
        dialog
    }

    pub fn set_current_cover(&self, cover: &gdk::Texture) {
        let imp = self.imp();
        if imp.cover.borrow().is_none() {
            imp.cover_picture.set_paintable(Some(cover));
            imp.cover_picture.set_visible(true);
        }
    }

    pub fn connect_save<F>(&self, save_fun: F)
    where
        F: Fn(PlaylistForm) -> oneshot::Receiver<()> + 'static,
    {
        self.imp().save_button.connect_clicked(clone!(
            #[weak(rename_to = dialog)]
            self,
            move |_| {
                dialog.set_busy(true);
                let saved = save_fun(dialog.form());
                glib::spawn_future_local(async move {
                    if saved.await.is_ok() {
                        dialog.close();
                    } else {
                        dialog.set_busy(false);
                        dialog.imp().error_label.set_visible(true);
                    }
                });
            }
        ));
    }

    fn set_busy(&self, busy: bool) {
        let imp = self.imp();
        imp.details.set_sensitive(!busy);
        imp.public_switch.set_sensitive(!busy);
        imp.save_button
            .set_sensitive(!busy && !imp.name_entry.text().trim().is_empty());
        imp.save_stack
            .set_visible_child_name(if busy { "spinner" } else { "label" });
        if busy {
            imp.error_label.set_visible(false);
        }
    }

    fn form(&self) -> PlaylistForm {
        let imp = self.imp();
        let buffer = imp.description_view.buffer();
        let description = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        PlaylistForm {
            name: imp.name_entry.text().trim().to_string(),
            description: Some(description).filter(|d| !d.is_empty()),
            public: imp.public_switch.is_active(),
            public_changed: imp.public_switch.is_active() != imp.initial_public.get(),
            cover: imp.cover.borrow().clone(),
        }
    }

    fn choose_cover(&self) {
        let filter = gtk::FileFilter::new();
        // translators: Name of the file filter of the dialog picking a playlist's cover.
        filter.set_name(Some(&gettext("Images")));
        filter.add_pixbuf_formats();
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);

        let file_dialog = gtk::FileDialog::builder()
            // translators: Title of the dialog picking a playlist's cover.
            .title(gettext("Choose Cover"))
            .filters(&filters)
            .modal(true)
            .build();
        let window = self.root().and_downcast::<gtk::Window>();
        file_dialog.open(
            window.as_ref(),
            gio::Cancellable::NONE,
            clone!(
                #[weak(rename_to = dialog)]
                self,
                move |file| {
                    if let Ok(file) = file {
                        glib::spawn_future_local(async move { dialog.load_cover(file).await });
                    }
                }
            ),
        );
    }

    async fn load_cover(&self, file: gio::File) {
        let contents = match file.load_contents_future().await {
            Ok((contents, _)) => contents.to_vec(),
            Err(e) => {
                warn!("playlist dialog: could not read the cover: {e}");
                return;
            }
        };
        match gdk::Texture::from_bytes(&glib::Bytes::from(&contents)) {
            Ok(texture) => {
                let imp = self.imp();
                imp.cover_picture.set_paintable(Some(&texture));
                imp.cover_picture.set_visible(true);
                imp.cover.replace(Some(contents));
            }
            Err(e) => warn!("playlist dialog: could not decode the cover: {e}"),
        }
    }
}
