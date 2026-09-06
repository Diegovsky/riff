//! Conversions for the playback queue.

use riff_api::models::Queue;

use super::Track;

/// Flatten a data-layer [`Queue`] into the app's ordered track list: the
/// currently-playing track (if any) first, followed by the upcoming items.
pub fn queue_songs(q: Queue) -> Vec<Track> {
    let mut songs = Vec::with_capacity(q.items.len() + 1);
    if let Some(current) = q.currently_playing {
        songs.push(current);
    }
    songs.extend(q.items);
    songs
}
