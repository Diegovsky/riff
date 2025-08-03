use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use libadwaita::prelude::AdwDialogExt;
use std::rc::Rc;

use super::album_header::AlbumHeaderWidget;
use super::release_details::ReleaseDetailsDialog;
use super::DetailsModel;

use crate::app::components::{
    Component, EventListener, HeaderBarComponent, HeaderBarWidget, Playlist, ScrollingHeaderWidget,
};
use crate::app::dispatch::Worker;
use crate::app::loader::ImageLoader;
use crate::app::state::PlaybackEvent;
use crate::app::{AppEvent, BrowserEvent};

mod imp {

    use libadwaita::subclass::prelude::BinImpl;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/details.ui")]
    pub struct AlbumDetailsWidget {
        #[template_child]
        pub scrolling_header: TemplateChild<ScrollingHeaderWidget>,

        #[template_child]
        pub headerbar: TemplateChild<HeaderBarWidget>,

        #[template_child]
        pub header_widget: TemplateChild<AlbumHeaderWidget>,

        #[template_child]
        pub album_tracks: TemplateChild<gtk::ListView>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AlbumDetailsWidget {
        const NAME: &'static str = "AlbumDetailsWidget";
        type Type = super::AlbumDetailsWidget;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AlbumDetailsWidget {
        fn constructed(&self) {
            self.parent_constructed();
            // self.header_mobile.set_centered();
            self.headerbar.add_classes(&["details__headerbar"]);
        }
    }

    impl WidgetImpl for AlbumDetailsWidget {}
    impl BinImpl for AlbumDetailsWidget {}
}

glib::wrapper! {
    pub struct AlbumDetailsWidget(ObjectSubclass<imp::AlbumDetailsWidget>) @extends gtk::Widget, libadwaita::Bin;
}

impl AlbumDetailsWidget {
    fn new() -> Self {
        glib::Object::new()
    }

    fn set_header_visible(&self, visible: bool) {
        let widget = self.imp();
        widget.headerbar.set_title_visible(true);
        if visible {
            widget.headerbar.add_classes(&["flat"]);
        } else {
            widget.headerbar.remove_classes(&["flat"]);
        }
    }

    fn connect_header(&self) {
        self.set_header_visible(false);
        self.imp()
            .scrolling_header
            .connect_header_visibility(clone!(
                #[weak(rename_to = _self)]
                self,
                move |visible| {
                    _self.set_header_visible(visible);
                }
            ));
    }

    fn connect_bottom_edge<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp().scrolling_header.connect_bottom_edge(f);
    }

    fn headerbar_widget(&self) -> &HeaderBarWidget {
        self.imp().headerbar.as_ref()
    }

    fn album_tracks_widget(&self) -> &gtk::ListView {
        self.imp().album_tracks.as_ref()
    }

    fn set_loaded(&self) {
        self.imp()
            .scrolling_header
            .add_css_class("container--loaded");
    }

    fn connect_liked<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp().header_widget.connect_liked(f);
    }

    fn connect_play<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp().header_widget.connect_play(f);
    }

    fn connect_info<F>(&self, f: F)
    where
        F: Fn(&Self) + 'static,
    {
        self.imp().header_widget.connect_info(clone!(
            #[weak(rename_to = _self)]
            self,
            move || f(&_self)
        ));
    }

    fn set_liked(&self, is_liked: bool) {
        self.imp().header_widget.set_liked(is_liked);
    }

    fn set_playing(&self, is_playing: bool) {
        self.imp().header_widget.set_playing(is_playing);
    }

    fn set_album_and_artist_and_year(&self, album: &str, artist: &str, year: Option<u32>) {
        self.imp()
            .header_widget
            .set_album_and_artist_and_year(album, artist, year);
        self.imp().headerbar.set_title_and_subtitle(album, artist);
    }

    fn set_artwork(&self, art: &gdk_pixbuf::Pixbuf) {
        self.imp().header_widget.set_artwork(art);
    }

    fn connect_artist_clicked<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp().header_widget.connect_artist_clicked(f);
    }
}

pub struct Details {
    model: Rc<DetailsModel>,
    worker: Worker,
    widget: AlbumDetailsWidget,
    modal: ReleaseDetailsDialog,
    children: Vec<Box<dyn EventListener>>,
}

impl Details {
    pub fn new(model: Rc<DetailsModel>, worker: Worker) -> Self {
        if model.get_album_info().is_none() {
            model.load_album_info();
        }

        let widget = AlbumDetailsWidget::new();

        let playlist = Box::new(Playlist::new(
            widget.album_tracks_widget().clone(),
            model.clone(),
            worker.clone(),
        ));

        let headerbar_widget = widget.headerbar_widget();
        let headerbar = Box::new(HeaderBarComponent::new(
            headerbar_widget.clone(),
            model.to_headerbar_model(),
        ));

        let modal = ReleaseDetailsDialog::new();

        widget.connect_liked(clone!(
            #[weak]
            model,
            move || model.toggle_save_album()
        ));

        widget.connect_play(clone!(
            #[weak]
            model,
            move || model.toggle_play_album()
        ));

        widget.connect_header();

        widget.connect_bottom_edge(clone!(
            #[weak]
            model,
            move || {
                model.load_more();
            }
        ));

        widget.connect_info(clone!(
            #[weak]
            modal,
            move |w| {
                let modal = modal.upcast_ref::<libadwaita::Dialog>();
                let parent = w.root().and_then(|r| r.downcast::<gtk::Window>().ok());
                modal.present(parent.as_ref());
            }
        ));

        Self {
            model,
            worker,
            widget,
            modal,
            children: vec![playlist, headerbar],
        }
    }

    fn update_liked(&self) {
        if let Some(info) = self.model.get_album_info() {
            let is_liked = info.description.is_liked;
            self.widget.set_liked(is_liked);
            self.widget.set_liked(is_liked);
        }
    }

    fn update_playing(&self, is_playing: bool) {
        if !self.model.album_is_playing() || !self.model.is_playing() {
            self.widget.set_playing(false);
            return;
        }
        self.widget.set_playing(is_playing);
    }

    fn update_details(&mut self) {
        if let Some(album) = self.model.get_album_info() {
            let details = &album.release_details;
            let album = &album.description;

            self.widget.set_liked(album.is_liked);

            self.widget.set_album_and_artist_and_year(
                &album.title[..],
                &album.artists_name(),
                album.year(),
            );

            self.widget.connect_artist_clicked(clone!(
                #[weak(rename_to = model)]
                self.model,
                move || model.view_artist()
            ));

            self.modal.set_details(
                &album.title,
                &album.artists_name(),
                &details.label,
                album.release_date.as_ref().unwrap(),
                details.total_tracks,
                &details.copyright_text,
            );

            if let Some(art) = album.art.clone() {
                let widget = self.widget.downgrade();

                self.worker.send_local_task(async move {
                    let pixbuf = ImageLoader::new()
                        .load_remote(&art[..], "jpg", 320, 320)
                        .await;
                    if let (Some(widget), Some(ref pixbuf)) = (widget.upgrade(), pixbuf) {
                        widget.set_artwork(pixbuf);
                        widget.set_loaded();
                    }
                });
            } else {
                self.widget.set_loaded();
            }
        }
    }
}

impl Component for Details {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.widget.upcast_ref()
    }

    fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
        Some(&mut self.children)
    }
}

impl EventListener for Details {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::BrowserEvent(BrowserEvent::AlbumDetailsLoaded(id))
                if id == &self.model.id =>
            {
                self.update_details();
                self.update_playing(true);
            }
            AppEvent::BrowserEvent(BrowserEvent::AlbumSaved(id))
            | AppEvent::BrowserEvent(BrowserEvent::AlbumUnsaved(id))
                if id == &self.model.id =>
            {
                self.update_liked();
            }
            AppEvent::PlaybackEvent(PlaybackEvent::PlaybackPaused) => {
                self.update_playing(false);
            }
            AppEvent::PlaybackEvent(PlaybackEvent::PlaybackResumed) => {
                self.update_playing(true);
            }
            _ => {}
        }
        self.broadcast_event(event);
    }
}
