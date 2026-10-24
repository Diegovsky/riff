use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use std::cell::RefCell;

use crate::app::components::display_add_css_provider;
use crate::app::models::PlaylistSummary;

/// Id of the menu item the picker is placed in (see `build_song_menu`).
pub const PLAYLIST_PICKER_MENU_ID: &str = "add-to-playlist";

const MAX_LISTED: usize = 5;

thread_local! {
    /// Ids of the playlists tracks were added to this session, latest first.
    static RECENT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn note_used(id: &str) {
    RECENT.with_borrow_mut(|recent| {
        recent.retain(|r| r != id);
        recent.insert(0, id.to_string());
    });
}

fn listed<'a>(
    playlists: &'a [PlaylistSummary],
    recent: &[String],
    query: &str,
) -> Vec<&'a PlaylistSummary> {
    let query = query.trim().to_lowercase();
    let rank = |p: &PlaylistSummary| recent.iter().position(|r| r == &p.id).unwrap_or(usize::MAX);
    let mut matches: Vec<&PlaylistSummary> = playlists
        .iter()
        .filter(|p| p.title.to_lowercase().contains(&query))
        .collect();
    matches.sort_by_key(|p| rank(p));
    matches.truncate(MAX_LISTED);
    matches
}

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/playlist_picker.ui")]
    pub struct PlaylistPicker {
        #[template_child]
        pub entry: TemplateChild<gtk::SearchEntry>,

        #[template_child]
        pub list: TemplateChild<gtk::Box>,

        #[template_child]
        pub empty: TemplateChild<gtk::Label>,

        pub playlists: RefCell<Vec<PlaylistSummary>>,

        pub on_picked: RefCell<Option<Box<dyn Fn(&PlaylistSummary)>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlaylistPicker {
        const NAME: &'static str = "PlaylistPicker";
        type Type = super::PlaylistPicker;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PlaylistPicker {
        fn constructed(&self) {
            self.parent_constructed();
            display_add_css_provider(resource!("/components/playlist_picker.css"));

            self.entry.connect_search_changed(clone!(
                #[weak(rename_to = imp)]
                self,
                move |_| imp.obj().fill()
            ));
            self.entry.connect_activate(clone!(
                #[weak(rename_to = imp)]
                self,
                move |_| {
                    let first = imp.obj().listed().into_iter().next();
                    if let Some(playlist) = first {
                        imp.obj().pick(&playlist);
                    }
                }
            ));
        }
    }

    impl WidgetImpl for PlaylistPicker {}
    impl BoxImpl for PlaylistPicker {}
}

glib::wrapper! {
    pub struct PlaylistPicker(ObjectSubclass<imp::PlaylistPicker>) @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl PlaylistPicker {
    pub fn new<F>(playlists: Vec<PlaylistSummary>, on_picked: F) -> Self
    where
        F: Fn(&PlaylistSummary) + 'static,
    {
        let picker: Self = glib::Object::new();
        picker.imp().playlists.replace(playlists);
        picker.imp().on_picked.replace(Some(Box::new(on_picked)));
        picker.fill();
        picker
    }

    fn listed(&self) -> Vec<PlaylistSummary> {
        let imp = self.imp();
        let playlists = imp.playlists.borrow();
        RECENT.with_borrow(|recent| {
            listed(&playlists, recent, &imp.entry.text())
                .into_iter()
                .cloned()
                .collect()
        })
    }

    fn fill(&self) {
        let imp = self.imp();
        while let Some(child) = imp.list.first_child() {
            imp.list.remove(&child);
        }
        let listed = self.listed();
        imp.empty.set_visible(listed.is_empty());
        for playlist in listed {
            let label = gtk::Label::builder()
                .label(&playlist.title)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .max_width_chars(28)
                .build();
            let button = gtk::Button::builder().child(&label).build();
            button.add_css_class("flat");
            button.connect_clicked(clone!(
                #[weak(rename_to = picker)]
                self,
                move |_| picker.pick(&playlist)
            ));
            imp.list.append(&button);
        }
    }

    fn pick(&self, playlist: &PlaylistSummary) {
        note_used(&playlist.id);
        if let Some(on_picked) = self.imp().on_picked.borrow().as_ref() {
            on_picked(playlist);
        }
        if let Some(popover) = self
            .ancestor(gtk::Popover::static_type())
            .and_downcast::<gtk::Popover>()
        {
            popover.popdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playlists(titles: &[&str]) -> Vec<PlaylistSummary> {
        titles
            .iter()
            .map(|t| PlaylistSummary {
                id: t.to_lowercase(),
                title: t.to_string(),
            })
            .collect()
    }

    fn titles(listed: Vec<&PlaylistSummary>) -> Vec<&str> {
        listed.into_iter().map(|p| p.title.as_str()).collect()
    }

    #[test]
    fn lists_the_first_few_playlists_without_a_query() {
        let all = playlists(&["A", "B", "C", "D", "E", "F", "G"]);
        assert_eq!(titles(listed(&all, &[], "")), ["A", "B", "C", "D", "E"]);
    }

    #[test]
    fn lists_recently_used_playlists_first() {
        let all = playlists(&["A", "B", "C", "D", "E", "F", "G"]);
        let recent = ["g".to_string(), "c".to_string()];
        assert_eq!(titles(listed(&all, &recent, "")), ["G", "C", "A", "B", "D"]);
    }

    #[test]
    fn search_reaches_playlists_beyond_those_listed() {
        let all = playlists(&["Jazz", "Rock", "Pop", "Folk", "Blues", "Road Trip"]);
        assert_eq!(titles(listed(&all, &[], " ro")), ["Rock", "Road Trip"]);
        assert!(listed(&all, &[], "metal").is_empty());
    }
}
