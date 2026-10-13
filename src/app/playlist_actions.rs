use std::collections::HashSet;

use riff_api::models::{is_resource_url, ImageSet};
use riff_api::{ApiService, DomainError, Load};

use crate::app::models::Track;
use crate::app::state::AppState;
use crate::app::AppAction;
use crate::feature_flags::{is_enabled, FeatureFlag};
use crate::settings;

pub struct TracksAdded {
    pub tracks: Vec<Track>,
    pub skipped: usize,
}

pub async fn unfollow_and_unpin_playlist(
    api: &ApiService,
    id: String,
    user_id: Option<String>,
) -> Result<AppAction, DomainError> {
    api.unfollow_playlist(&id).await?;
    if let Some(user_id) = user_id.filter(|_| is_enabled(FeatureFlag::PinnedObjects)) {
        settings::unpin_object(&user_id, settings::PinnedKind::Playlist, &id);
    }
    Ok(AppAction::RemovePlaylist(id))
}

pub async fn add_tracks_to_playlist(
    api: &ApiService,
    id: &str,
    mut tracks: Vec<Track>,
    load: Load,
) -> Result<TracksAdded, DomainError> {
    tracks.retain(|track| track.rri.uri.is_some());
    let asked = tracks.len();
    let uris: Vec<String> = tracks.iter().filter_map(|t| t.rri.uri.clone()).collect();
    let mut added: HashSet<String> = api
        .add_to_playlist(id, uris, load)
        .await?
        .into_iter()
        .collect();
    tracks.retain(|track| track.rri.uri.as_ref().is_some_and(|uri| added.remove(uri)));
    Ok(TracksAdded {
        skipped: asked - tracks.len(),
        tracks,
    })
}

pub fn playlist_title(state: &AppState, id: &str) -> Option<String> {
    let playlists = &state.logged_user.playlists;
    let playlist = playlists.iter().find(|playlist| playlist.id == id)?;
    Some(playlist.title.clone())
}

pub fn playlist_lacks_cover(state: &AppState, id: &str) -> bool {
    state
        .browser
        .home_state()
        .and_then(|home| home.playlists.iter().find(|card| card.id() == id))
        .is_some_and(|card| card.image().is_none_or(|url| is_resource_url(&url)))
}

pub async fn upload_playlist_cover(
    api: &ApiService,
    id: String,
    cover: Vec<u8>,
    load: Load,
) -> Result<Vec<AppAction>, DomainError> {
    let art = api.upload_playlist_cover(&id, cover, load).await?;
    Ok(cover_actions(id, art))
}

pub async fn refresh_playlist_cover(api: &ApiService, id: String, poll: bool) -> Vec<AppAction> {
    let cover = api
        .refreshed_playlist_cover(&id, poll, Load::background())
        .await;
    cover_actions(id, cover)
}

fn cover_actions(id: String, cover: Option<ImageSet>) -> Vec<AppAction> {
    cover
        .map(|art| AppAction::UpdatePlaylistArt(id, art))
        .into_iter()
        .collect()
}
