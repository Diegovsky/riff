use gettextrs::*;

use crate::app::components::navigation_panel::NavigationPanelDestination;
use crate::app::state::Route;

lazy_static! {
    // translators: This is part of a contextual menu attached to a single track; this entry allows viewing the album containing a specific track.
    pub static ref VIEW_ALBUM: String = gettext("View Album");

    // translators: This is part of a contextual menu attached to a single track; the intent is to copy the link (public URL) to a specific track.
    pub static ref COPY_LINK: String = gettext("Copy Link");

    // translators: This is part of a contextual menu attached to a single track; this entry queues the track after the tracks already queued.
    pub static ref ADD_TO_QUEUE: String = gettext("Add to Queue");

    // translators: This is part of a contextual menu attached to a single track; this entry opens a list of playlists to add the track to.
    pub static ref ADD_TO_PLAYLIST: String = gettext("Add to Playlist");

    // translators: This is part of a contextual menu attached to a single track; this entry starts selecting tracks, with this one.
    pub static ref SELECT: String = gettext("Select Track");

    // translators: This is part of a contextual menu attached to a single track; this entry removes the track from the playlist being viewed.
    pub static ref REMOVE_FROM_PLAYLIST: String = gettext("Remove from Playlist");

    // translators: Name of the user's saved tracks, as the source of what's playing (e.g. "Next from: Saved Tracks").
    pub static ref SAVED_TRACKS: String = gettext("Saved Tracks");

    // translators: Header above the tracks the user added to the play queue, on the queue page.
    pub static ref QUEUE_NEXT_IN_QUEUE: String = gettext("Next in Queue");

    // translators: Header of the current track, on the queue page (when shown in the list).
    pub static ref QUEUE_NOW_PLAYING: String = gettext("Now Playing");

    // translators: Tooltip of the button in the header of the queued tracks, on the queue page. It removes all queued tracks.
    pub static ref CLEAR_QUEUE: String = gettext("Clear Queue");

    // translators: Notification shown after tracks were added to the play queue.
    pub static ref ADDED_TO_QUEUE: String = gettext("Added to queue");

    // translators: This is part of a contextual menu attached to a single track; this entry removes a track from the play queue.
    pub static ref REMOVE_FROM_QUEUE: String = gettext("Remove from Queue");

    // translators: This is part of a contextual menu attached to a single track; this entry saves the track to the user's saved tracks.
    pub static ref LIKE: String = gettext("Add to Saved Tracks");

    // translators: This is part of a contextual menu attached to a single track; this entry removes the track from the user's saved tracks.
    pub static ref UNLIKE: String = gettext("Remove from Saved Tracks");

    // translators: This is part of a contextual menu attached to a single track, and the tooltip of the pin button on a detail page; this entry pins the item to the sidebar.
    pub static ref PIN_TO_SIDEBAR: String = gettext("Pin to Sidebar");

    // translators: This is part of a contextual menu attached to a single track, and the tooltip of the pin button on a detail page; this entry removes the item from the sidebar.
    pub static ref UNPIN_FROM_SIDEBAR: String = gettext("Unpin from Sidebar");

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

    // translators: This is part of the menu of a playlist's page; this entry opens a dialog to edit the name, description, cover and visibility of a playlist owned by the user.
    pub static ref EDIT_PLAYLIST_DETAILS: String = gettext("Edit Details");

    // translators: This is the caption shown in the header of a playlist's detail page when anyone can open the playlist.
    pub static ref PUBLIC_PLAYLIST_CAPTION: String = gettext("Public Playlist");

    // translators: This is the caption shown in the header of a playlist's detail page when nobody but its owner can open the playlist.
    pub static ref PRIVATE_PLAYLIST_CAPTION: String = gettext("Private Playlist");

    // translators: This is the caption shown in the header of an album's detail page when the release is a full album.
    pub static ref ALBUM_CAPTION: String = gettext("Album");

    // translators: This is the caption shown in the header of an album's detail page when the release is a single.
    pub static ref SINGLE_CAPTION: String = gettext("Single");

    // translators: This is the caption shown in the header of an album's detail page when the release is an EP (extended play).
    pub static ref EP_CAPTION: String = gettext("EP");

    // translators: Shown in place of a card list when the selected filter (e.g. "EPs") matches nothing.
    pub static ref NO_FILTER_RESULTS: String = gettext("No items found for this filter");

    // translators: This is the caption shown in the header of an album's detail page when the release is a compilation.
    pub static ref COMPILATION_CAPTION: String = gettext("Compilation");
}

pub fn added_to_playlist_label(playlist: &str) -> String {
    // translators: This notification is shown when a track was added to a playlist; {} is the name of the playlist.
    fill(gettext("Added to {}"), &[playlist])
}

pub fn already_in_playlist_label(playlist: &str) -> String {
    // translators: This notification is shown when nothing was added to a playlist, as it already lists the tracks; {} is the name of the playlist.
    fill(gettext("Already in {}"), &[playlist])
}

pub fn added_to_playlist_skipping_label(playlist: &str, skipped: usize) -> String {
    let text = ngettext(
        // translators: This notification is shown when tracks were added to a playlist, except those it already lists; the first {} is the name of the playlist, the second {} is how many tracks were left out.
        "Added to {}, skipping {} track already in it",
        "Added to {}, skipping {} tracks already in it",
        skipped as u32,
    );
    fill(text, &[playlist, &skipped.to_string()])
}

pub fn n_tracks_selected_label(n: usize) -> String {
    // translators: This shows up when in selection mode. This text should be as short as possible.
    let text = ngettext("{} track selected", "{} tracks selected", n as u32);
    fill(text, &[&n.to_string()])
}

// Fills the `{}` of a translated text in order. xgettext only finds the
// literal passed straight to `gettext`, so format with this instead of a macro.
pub fn fill(text: String, args: &[&str]) -> String {
    let mut parts = text.split("{}");
    let mut filled = parts.next().unwrap_or_default().to_string();
    for (i, part) in parts.enumerate() {
        filled.push_str(args.get(i).copied().unwrap_or_default());
        filled.push_str(part);
    }
    filled
}

fn fill_track(markup: String, title: &str, artist: &str) -> String {
    let title = glib::markup_escape_text(title);
    let artist = glib::markup_escape_text(artist);
    fill(markup, &[&title, &artist])
}

pub fn now_playing_markup(title: &str, artist: &str) -> String {
    // translators: Shown from time to time in the bar at the bottom of the window that opens the queue. It's Pango markup: keep the tags, around the same words. The first {} is the title of the track playing, the second its artist.
    let markup = gettext("<span size=\"small\" weight=\"light\">Now Playing</span> <b>{}</b> <span size=\"small\" weight=\"light\">by</span> {}");
    fill_track(markup, title, artist)
}

pub fn current_track_markup(title: &str, artist: &str) -> String {
    // translators: Shown from time to time in the bar at the bottom of the window that opens the queue, while playback is paused. It's Pango markup: keep the tags, around the same words. The first {} is the title of the current track, the second its artist.
    let markup = gettext("<span size=\"small\" weight=\"light\">Current Track</span> <b>{}</b> <span size=\"small\" weight=\"light\">by</span> {}");
    fill_track(markup, title, artist)
}

pub fn up_next_markup(title: &str, artist: &str) -> String {
    // translators: Shown from time to time in the bar at the bottom of the window that opens the queue. It's Pango markup: keep the tags, around the same words. The first {} is the title of the track playing next, the second its artist.
    let markup = gettext("<span size=\"small\" weight=\"light\">Up Next</span> <b>{}</b> <span size=\"small\" weight=\"light\">by</span> {}");
    fill_track(markup, title, artist)
}

pub fn queue_context_label(name: &str) -> String {
    // translators: Header above the rest of the album, playlist or artist being played, on the queue page. {} is its name.
    fill(gettext("Next from: {}"), &[name])
}

pub fn shuffle_separation_label(n: u32) -> String {
    let text = ngettext(
        // translators: Description of the shuffle separation preference. {} is the number of other songs, chosen by the user. "When possible" because an album or playlist can have too few songs.
        "When possible, shuffle and repeat mode will play at least {} other song before repeating a song",
        "When possible, shuffle and repeat mode will play at least {} other songs before repeating a song",
        n,
    );
    fill(text, &[&n.to_string()])
}

pub fn more_from_label(artist: &str) -> String {
    // translators: This is part of a contextual menu attached to a single track; the full text is "More from <artist>".
    fill(gettext("More from {}"), &[artist])
}

pub fn album_by_artist_label(album: &str, artist: &str) -> String {
    // translators: This is part of a larger label that reads "<Album> by <Artist>"
    fill(gettext("{} by {}"), &[album, artist])
}

pub fn route_label(route: &Route) -> String {
    match route {
        Route::Home(page) => NavigationPanelDestination::from_home_page(*page).title(),
        // translators: An album in the back/forward history menu, while its name is loading.
        Route::Album(_) => gettext("Album"),
        // translators: An artist in the back/forward history menu, while their name is loading.
        Route::Artist(_) => gettext("Artist"),
        // translators: A playlist in the back/forward history menu, while its name is loading.
        Route::Playlist(_) => gettext("Playlist"),
        // translators: A user's page in the back/forward history menu, while their name is loading.
        Route::User(_) => gettext("User"),
        // translators: The search page in the back/forward history menu.
        Route::Search => gettext("Search"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formatted_labels() {
        assert!(shuffle_separation_label(1).contains("at least 1 other song before"));
        assert!(shuffle_separation_label(3).contains("at least 3 other songs before"));
        for markup in [
            now_playing_markup("Rock & Roll", "<Band>"),
            current_track_markup("Rock & Roll", "<Band>"),
            up_next_markup("Rock & Roll", "<Band>"),
        ] {
            let (_, text, _) = gtk::pango::parse_markup(&markup, '\0').unwrap();
            assert!(text.contains("Rock & Roll") && text.contains("<Band>"));
        }
        assert_eq!(queue_context_label("{} & Co"), "Next from: {} & Co");
        assert_eq!(more_from_label("Guns N' Roses"), "More from Guns N' Roses");
        assert_eq!(
            album_by_artist_label("Don't Look Back", "Guns N' Roses"),
            "Don't Look Back by Guns N' Roses"
        );
        assert_eq!(
            now_playing_markup("{}", "<b>"),
            "<span size=\"small\" weight=\"light\">Now Playing</span> <b>{}</b> <span size=\"small\" weight=\"light\">by</span> &lt;b&gt;"
        );
    }
}
