#![allow(clippy::all)]

use gio::prelude::*;
use glib::subclass::prelude::*;
use glib::Properties;

glib::wrapper! {
    pub struct CardModel(ObjectSubclass<imp::CardModel>);
}

impl CardModel {
    pub fn new(id: &str, image: Option<&String>, title: &str, subtitle: &str) -> CardModel {
        glib::Object::builder()
            .property("id", id)
            .property("image", &image)
            .property("title", title)
            .property("subtitle", subtitle)
            .build()
    }
}

mod imp {
    use super::*;
    use std::cell::RefCell;

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
        AlbumDescription, ArtistRef, ArtistSummary, ImageSet, PlaylistDescription, SongBatch, UserRef,
    };

    #[test]
    fn test_new_with_all_fields() {
        let img = "https://example.com/img.jpg".to_string();
        let card = CardModel::new("abc123", Some(&img), "My Title", "My Subtitle");
        assert_eq!(card.id(), "abc123");
        assert_eq!(card.image(), Some(img));
        assert_eq!(card.title(), "My Title");
        assert_eq!(card.subtitle(), "My Subtitle");
    }

    #[test]
    fn test_new_without_image() {
        let card = CardModel::new("id1", None, "Title", "Sub");
        assert_eq!(card.image(), None);
    }

    #[test]
    fn test_set_properties() {
        let card = CardModel::new("id", None, "old", "old");
        card.set_title("new title".to_string());
        card.set_subtitle("new sub".to_string());
        assert_eq!(card.title(), "new title");
        assert_eq!(card.subtitle(), "new sub");
    }

    #[test]
    fn test_from_album_description() {
        let album = AlbumDescription {
            id: "album1".to_string(),
            title: "Album Title".to_string(),
            artists: vec![
                ArtistRef { id: "a1".to_string(), name: "Artist A".to_string() },
                ArtistRef { id: "a2".to_string(), name: "Artist B".to_string() },
            ],
            release_date: Some("2023-05-01".to_string()),
            art: ImageSet::from_images(vec![(Some(300), "https://img.com/cover.jpg".to_string())]),
            songs: SongBatch::empty(),
            is_liked: false,
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
            owner: UserRef { id: "user1".to_string(), display_name: "John".to_string() },
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
            photo: ImageSet::from_images(vec![(Some(300), "https://img.com/photo.jpg".to_string())]),
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
        };
        let card = CardModel::from(&artist);
        assert_eq!(card.image(), None);
    }
}
