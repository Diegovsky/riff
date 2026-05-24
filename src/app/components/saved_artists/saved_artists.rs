use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use std::rc::Rc;

use super::SavedArtistsModel;
use crate::app::components::utils::wrap_flowbox_item;
use crate::app::components::{ArtistWidget, Component, EventListener};
use crate::app::dispatch::Worker;
use crate::app::models::ArtistModel;
use crate::app::state::LoginEvent;
use crate::app::{AppEvent, BrowserEvent};

mod imp {

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/saved_artists.ui")]
    pub struct SavedArtistsWidget {
        #[template_child]
        pub scrolled_window: TemplateChild<gtk::ScrolledWindow>,

        #[template_child]
        pub flowbox: TemplateChild<gtk::FlowBox>,

        #[template_child]
        pub status_page: TemplateChild<libadwaita::StatusPage>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SavedArtistsWidget {
        const NAME: &'static str = "SavedArtistsWidget";
        type Type = super::SavedArtistsWidget;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SavedArtistsWidget {}
    impl WidgetImpl for SavedArtistsWidget {}
    impl BoxImpl for SavedArtistsWidget {}
}

glib::wrapper! {
    pub struct SavedArtistsWidget(ObjectSubclass<imp::SavedArtistsWidget>) @extends gtk::Widget, gtk::Box;
}

impl Default for SavedArtistsWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl SavedArtistsWidget {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn connect_bottom_edge<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp()
            .scrolled_window
            .connect_edge_reached(move |_, pos| {
                if let gtk::PositionType::Bottom = pos {
                    f()
                }
            });
    }

    fn bind_artists<F>(&self, worker: Worker, store: &gio::ListStore, on_artist_pressed: F)
    where
        F: Fn(String) + Clone + 'static,
    {
        let store_clone = store.clone();
        self.imp()
            .flowbox
            .bind_model(Some(store), move |item| {
                wrap_flowbox_item(item, |artist_model: &ArtistModel| {
                    ArtistWidget::for_model(artist_model, worker.clone())
                })
            });
        self.imp()
            .flowbox
            .connect_child_activated(move |_, child| {
                let index = child.index() as u32;
                if let Some(item) = store_clone.item(index) {
                    if let Some(artist_model) = item.downcast_ref::<ArtistModel>() {
                        on_artist_pressed(artist_model.id());
                    }
                }
            });
    }

    pub fn status_page(&self) -> &libadwaita::StatusPage {
        &self.imp().status_page
    }
}

pub struct SavedArtists {
    widget: SavedArtistsWidget,
    worker: Worker,
    model: Rc<SavedArtistsModel>,
}

impl SavedArtists {
    pub fn new(worker: Worker, model: SavedArtistsModel) -> Self {
        let model = Rc::new(model);
        let widget = SavedArtistsWidget::new();

        widget.connect_bottom_edge(clone!(
            #[weak]
            model,
            move || {
                model.load_more_artists();
            }
        ));

        Self {
            widget,
            worker,
            model,
        }
    }

    fn bind_flowbox(&self) {
        if let Some(store) = self.model.get_list_store() {
            self.widget.bind_artists(
                self.worker.clone(),
                store.inner(),
                clone!(
                    #[weak(rename_to = model)]
                    self.model,
                    move |id| {
                        model.open_artist(id);
                    }
                ),
            );
        }
    }
}

impl EventListener for SavedArtists {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::Started => {
                self.bind_flowbox();
            }
            AppEvent::LoginEvent(LoginEvent::LoginCompleted) => {
                let _ = self.model.refresh_saved_artists();
            }
            AppEvent::BrowserEvent(BrowserEvent::SavedArtistsUpdated) => {
                self.widget
                    .status_page()
                    .set_visible(!self.model.has_artists());
            }
            _ => {}
        }
    }
}

impl Component for SavedArtists {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.widget.as_ref()
    }
}
