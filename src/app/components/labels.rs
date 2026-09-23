use gettextrs::*;

lazy_static! {
    // translators: This is part of a contextual menu attached to a single track; this entry allows viewing the album containing a specific track.
    pub static ref VIEW_ALBUM: String = gettext("View Album");

    // translators: This is part of a contextual menu attached to a single track; the intent is to copy the link (public URL) to a specific track.
    pub static ref COPY_LINK: String = gettext("Copy Link");

    // translators: This is part of a contextual menu attached to a single track; this entry adds a track at the end of the play queue.
    pub static ref ADD_TO_QUEUE: String = gettext("Add to Queue");

    // translators: This is part of a contextual menu attached to a single track; this entry removes a track from the play queue.
    pub static ref REMOVE_FROM_QUEUE: String = gettext("Remove from Queue");

    // translators: This is part of a contextual menu attached to a single track; this entry saves the track to the user's saved tracks.
    pub static ref LIKE: String = gettext("Add to Saved Tracks");

    // translators: This is part of a contextual menu attached to a single track; this entry removes the track from the user's saved tracks.
    pub static ref UNLIKE: String = gettext("Remove from Saved Tracks");

    // translators: This is the tooltip for the like/star button on an album's detail page; it saves the album to the user's saved albums.
    pub static ref LIKE_ALBUM: String = gettext("Add to Saved Albums");

    // translators: This is the tooltip for the like/star button on an album's detail page; it removes the album from the user's saved albums.
    pub static ref UNLIKE_ALBUM: String = gettext("Remove from Saved Albums");

    // translators: This is the tooltip for the like/star button on an artist's detail page; it follows the artist, adding them to the user's saved artists.
    pub static ref LIKE_ARTIST: String = gettext("Add to Saved Artists");

    // translators: This is the tooltip for the like/star button on an artist's detail page; it unfollows the artist, removing them from the user's saved artists.
    pub static ref UNLIKE_ARTIST: String = gettext("Remove from Saved Artists");

    // translators: This is the tooltip for the like/star button on a playlist's detail page; it saves the playlist to the user's saved playlists.
    pub static ref LIKE_PLAYLIST: String = gettext("Add to Saved Playlists");

    // translators: This is the tooltip for the like/star button on a playlist's detail page; it removes the playlist from the user's saved playlists.
    pub static ref UNLIKE_PLAYLIST: String = gettext("Remove from Saved Playlists");

    // translators: This is part of a contextual menu attached to a playlist in the sidebar; this entry starts playing the playlist from the first track.
    pub static ref PLAY: String = gettext("Play");

    // translators: This is part of a contextual menu attached to a playlist in the sidebar; this entry starts playing the playlist in shuffle mode.
    pub static ref SHUFFLE: String = gettext("Shuffle");

    // translators: This is part of a contextual menu attached to a playlist in the sidebar; this entry deletes a playlist owned by the user.
    pub static ref DELETE_PLAYLIST: String = gettext("Delete Playlist");

    // translators: This is part of a contextual menu attached to a playlist in the sidebar; this entry unfollows a playlist the user does not own.
    pub static ref UNFOLLOW_PLAYLIST: String = gettext("Unfollow Playlist");

    // translators: This is the caption shown in the header of an album's detail page when the release is a full album.
    pub static ref ALBUM_CAPTION: String = gettext("Album");

    // translators: This is the caption shown in the header of an album's detail page when the release is a single.
    pub static ref SINGLE_CAPTION: String = gettext("Single");

    // translators: This is the caption shown in the header of an album's detail page when the release is a compilation.
    pub static ref COMPILATION_CAPTION: String = gettext("Compilation");
}

pub fn add_to_playlist_label(playlist: &str) -> String {
    // this is just to fool xgettext, it doesn't like macros (or rust for that matter) :(
    if cfg!(debug_assertions) {
        // translators: This is part of a larger text that says "Add to <playlist name>". This text should be as short as possible.
        gettext("Add to {}");
    }
    gettext!("Add to {}", playlist)
}

pub fn n_tracks_selected_label(n: usize) -> String {
    // this is just to fool xgettext, it doesn't like macros (or rust for that matter) :(
    if cfg!(debug_assertions) {
        // translators: This shows up when in selection mode. This text should be as short as possible.
        ngettext("{} track selected", "{} tracks selected", n as u32);
    }
    ngettext!("{} track selected", "{} tracks selected", n as u32, n)
}

pub fn more_from_label(artist: &str) -> String {
    // this is just to fool xgettext, it doesn't like macros (or rust for that matter) :(
    if cfg!(debug_assertions) {
        // translators: This is part of a contextual menu attached to a single track; the full text is "More from <artist>".
        gettext("More from {}");
    }
    gettext!("More from {}", glib::markup_escape_text(artist))
}

pub fn album_by_artist_label(album: &str, artist: &str) -> String {
    // this is just to fool xgettext, it doesn't like macros (or rust for that matter) :(
    if cfg!(debug_assertions) {
        // translators: This is part of a larger label that reads "<Album> by <Artist>"
        gettext("{} by {}");
    }
    gettext!(
        "{} by {}",
        glib::markup_escape_text(album),
        glib::markup_escape_text(artist)
    )
}
