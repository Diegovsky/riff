#![allow(clippy::all)]

use gettextrs::gettext;
use gio::prelude::*;
use glib::subclass::prelude::*;
use glib::Properties;

use std::{
    any::Any,
    cell::{Cell, Ref, RefCell},
};

glib::wrapper! {
    pub struct CardModel(ObjectSubclass<imp::CardModel>);
}

impl CardModel {
    pub fn new(
        id: &str,
        image: Option<&String>,
        title: &str,
        subtitle: &str,
        release_date: Option<&str>,
        popularity: Option<u32>,
        insertion_position: Option<u32>,
    ) -> CardModel {
        let title = if title.is_empty() && !id.is_empty() {
            gettext("Untitled")
        } else {
            title.to_string()
        };
        let mut builder = glib::Object::builder()
            .property("id", id)
            .property("image", &image)
            .property("title", &title)
            .property("subtitle", subtitle);
        if let Some(rd) = release_date {
            builder = builder.property("release-date", rd);
        }
        if let Some(pop) = popularity {
            builder = builder.property("popularity", pop);
        }
        if let Some(pos) = insertion_position {
            builder = builder.property("insertion-position", pos);
        }
        builder.build()
    }

    pub fn with_data<T: Any>(self, data: T) -> Self {
        self.imp().data.borrow_mut().replace(Box::new(data));
        self
    }

    pub fn data(&self) -> Option<Ref<Box<dyn Any>>> {
        // I couldn't think of a batter way to transpose Ref<Option> into Option<Ref>
        let data = self.imp().data.borrow();
        if data.is_none() {
            return None;
        }
        Some(Ref::map(data, |data| data.as_ref().unwrap()))
    }
}

mod imp {

    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::CardModel)]
    pub struct CardModel {
        #[property(get, set)]
        id: RefCell<String>,
        #[property(get, set)]
        image: RefCell<Option<String>>,
        #[property(get, set)]
        title: RefCell<String>,
        #[property(get, set)]
        subtitle: RefCell<String>,
        #[property(get, set, name = "release-date")]
        release_date: RefCell<String>,
        #[property(get, set)]
        popularity: Cell<u32>,
        #[property(get, set, name = "insertion-position")]
        insertion_position: Cell<u32>,

        pub data: RefCell<Option<Box<dyn Any + 'static>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CardModel {
        const NAME: &'static str = "CardModel";
        type Type = super::CardModel;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for CardModel {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::models::{
        AlbumDescription, ArtistRef, ArtistSummary, ImageSet, PlaylistDescription, SongBatch,
        UserRef,
    };

    #[test]
    fn test_new_with_all_fields() {
        let img = "https://example.com/img.jpg".to_string();
        let card = CardModel::new(
            "abc123",
            Some(&img),
            "My Title",
            "My Subtitle",
            None,
            None,
            None,
        );
        assert_eq!(card.id(), "abc123");
        assert_eq!(card.image(), Some(img));
        assert_eq!(card.title(), "My Title");
        assert_eq!(card.subtitle(), "My Subtitle");
    }

    #[test]
    fn test_new_without_image() {
        let card = CardModel::new("id1", None, "Title", "Sub", None, None, None);
        assert_eq!(card.image(), None);
    }

    #[test]
    fn test_set_properties() {
        let card = CardModel::new("id", None, "old", "old", None, None, None);
        card.set_title("new title".to_string());
        card.set_subtitle("new sub".to_string());
        assert_eq!(card.title(), "new title");
        assert_eq!(card.subtitle(), "new sub");
    }

    #[test]
    fn test_release_date() {
        let card: CardModel = glib::Object::builder()
            .property("id", "id")
            .property("title", "T")
            .property("subtitle", "S")
            .property("release-date", "2023-05-01")
            .build();
        assert_eq!(card.release_date(), "2023-05-01");
    }

    #[test]
    fn test_popularity() {
        let card: CardModel = glib::Object::builder()
            .property("id", "id")
            .property("title", "T")
            .property("subtitle", "S")
            .property("popularity", 75u32)
            .build();
        assert_eq!(card.popularity(), 75);
    }

    #[test]
    fn test_from_album_description() {
        let album = AlbumDescription {
            id: "album1".to_string(),
            title: "Album Title".to_string(),
            artists: vec![
                ArtistRef {
                    id: "a1".to_string(),
                    name: "Artist A".to_string(),
                },
                ArtistRef {
                    id: "a2".to_string(),
                    name: "Artist B".to_string(),
                },
            ],
            release_date: Some("2023-05-01".to_string()),
            art: ImageSet::from_images(vec![(Some(300), "https://img.com/cover.jpg".to_string())]),
            songs: SongBatch::empty(),
            is_liked: false,
            popularity: 72,
        };
        let card = CardModel::from(&album);
        assert_eq!(card.id(), "album1");
        assert_eq!(card.title(), "Album Title");
        assert_eq!(card.subtitle(), "Artist A, Artist B");
        assert_eq!(card.image(), Some("https://img.com/cover.jpg".to_string()));
    }

    #[test]
    fn test_from_playlist_description() {
        let playlist = PlaylistDescription {
            id: "pl1".to_string(),
            title: "My Playlist".to_string(),
            art: None,
            songs: SongBatch::empty(),
            owner: UserRef {
                id: "user1".to_string(),
                display_name: "John".to_string(),
            },
        };
        let card = CardModel::from(&playlist);
        assert_eq!(card.id(), "pl1");
        assert_eq!(card.title(), "My Playlist");
        assert_eq!(card.subtitle(), "John");
        assert_eq!(card.image(), None);
    }

    #[test]
    fn test_from_artist_summary() {
        let artist = ArtistSummary {
            id: "art1".to_string(),
            name: "Cool Artist".to_string(),
            photo: ImageSet::from_images(vec![(
                Some(300),
                "https://img.com/photo.jpg".to_string(),
            )]),
            popularity: 85,
        };
        let card = CardModel::from(&artist);
        assert_eq!(card.id(), "art1");
        assert_eq!(card.title(), "Cool Artist");
        assert_eq!(card.subtitle(), "");
        assert_eq!(card.image(), Some("https://img.com/photo.jpg".to_string()));
    }

    #[test]
    fn test_from_artist_summary_no_photo() {
        let artist = ArtistSummary {
            id: "art2".to_string(),
            name: "No Photo".to_string(),
            photo: None,
            popularity: 0,
        };
        let card = CardModel::from(&artist);
        assert_eq!(card.image(), None);
    }
}
