// Widget for the album detail page.
// Shows album art, track list, and a release info dialog (triggered by the
// info button in the header).

use gtk::prelude::*;
use libadwaita::prelude::AdwDialogExt;
use std::rc::Rc;

use super::release_details::ReleaseDetailsDialog;
use super::DetailsModel;

use crate::app::components::{
    Component, DetailsPageComponent, EventListener, HasHeaderBarModel, HeaderRegistrar, PageModel,
};
use crate::app::dispatch::Worker;
use crate::app::AppEvent;

/// GTK widget for the album detail page.
pub struct Details {
    component: DetailsPageComponent<DetailsModel>,
    modal: ReleaseDetailsDialog,
}

impl Details {
    pub fn new(
        model: Rc<DetailsModel>,
        worker: Worker,
        registrar: HeaderRegistrar,
        name: String,
    ) -> Self {
        let mut component = DetailsPageComponent::new(
            model.clone(),
            model.to_headerbar_model(),
            worker,
            registrar,
            name,
        );
        component.create_playlist(None);

        let modal = ReleaseDetailsDialog::new();

        // Wire the info button to open the release details dialog.
        component.page().header().connect_info(clone!(
            #[weak]
            modal,
            #[weak(rename_to = widget)]
            component.page().widget(),
            move || {
                let modal = modal.upcast_ref::<libadwaita::Dialog>();
                let parent = widget.root().and_then(|r| r.downcast::<gtk::Window>().ok());
                modal.present(parent.as_ref());
            }
        ));

        Self { component, modal }
    }
}

impl Component for Details {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.component.get_root_widget()
    }
    fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
        self.component.get_children()
    }
}

impl EventListener for Details {
    fn on_event(&mut self, event: &AppEvent) {
        if self.component.handle_event(event) {
            // Update the release details dialog when album info loads
            if self.component.model().should_refresh_details(event) {
                if let Some(album) = self.component.model().get_album_info() {
                    let details = &album.release_details;
                    let desc = &album.description;
                    self.modal.set_details(
                        &desc.title,
                        &desc.artists_name(),
                        &details.label,
                        desc.release_date.as_ref().unwrap(),
                        details.total_tracks,
                        &details.copyright_text,
                    );
                }
            }
        }
        self.broadcast_event(event);
    }
}
