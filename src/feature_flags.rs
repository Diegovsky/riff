use gio::prelude::SettingsExt;

const SETTINGS: &str = "dev.diegovsky.Riff";

#[derive(Clone, Copy, Debug)]
pub enum FeatureFlag {
    /*
     The playlist select mode, which allows users to remove songs from the playlist or to add songs
     to the play queue, has visually bugged buttons in the page's header. It is questionable
     if the buttons should live in the page's header.
     */
    PlaylistEditMode,
    /*
     The album select mode allows users to select multiple songs at once to save to library, add to
     a playlist, or add songs to the queue. However, it has visually bugged buttons in the page's
     header. It is also questionable if the buttons should live in the page's header.
     */
    AlbumSelectMode,
    /*
     Creating new playlists workflow needs to be flushed out further before launching. Currently,
     users can create a new play list with no songs but interacting with the playlist is awkward.
     Furthermore, it is possible to crash the application by viewing a newly created playlist and
     then viewing another playlist in the same session.
     */
    CreateNewPlaylist,
    /*
    Users in the album select mode can re-order songs in the queue, save songs to the library, and
    remove songs from the queue. There are visual bugs relating to the header buttons, and it is
    possible to crash the application by re-ordering a song past the last queued song.
     */
    NowPlayingSelectMode,
}

impl FeatureFlag {
    pub const ALL: &[FeatureFlag] = &[
        FeatureFlag::PlaylistEditMode,
        FeatureFlag::AlbumSelectMode,
        FeatureFlag::CreateNewPlaylist,
        FeatureFlag::NowPlayingSelectMode,
    ];

    pub fn key(&self) -> &'static str {
        match self {
            FeatureFlag::PlaylistEditMode => "feature-playlist-edit-mode",
            FeatureFlag::AlbumSelectMode => "feature-album-select-mode",
            FeatureFlag::CreateNewPlaylist => "feature-create-new-playlist",
            FeatureFlag::NowPlayingSelectMode => "feature-now-playing-select-mode",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            FeatureFlag::PlaylistEditMode => "Playlist Edit Mode",
            FeatureFlag::AlbumSelectMode => "Album Select Mode",
            FeatureFlag::CreateNewPlaylist => "Create New Playlist",
            FeatureFlag::NowPlayingSelectMode => "Now Playing Select Mode",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            FeatureFlag::PlaylistEditMode => {
                "Enable the edit button to rename playlists and manage tracks."
            }
            FeatureFlag::AlbumSelectMode => {
                "Enable the selection checkmark when viewing an album."
            }
            FeatureFlag::CreateNewPlaylist => {
                "Enable the New Playlist button in the sidebar."
            }
            FeatureFlag::NowPlayingSelectMode => {
                "Enable selection mode on the Now Playing page."
            }
        }
    }
}

pub fn is_enabled(flag: FeatureFlag) -> bool {
    let settings = gio::Settings::new(SETTINGS);
    settings.boolean(flag.key())
}
