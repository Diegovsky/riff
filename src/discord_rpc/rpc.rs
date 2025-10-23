use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::Mutex;

static DISCORD_RPC: Mutex<Option<DiscordRPC>> = Mutex::new(None);

pub struct DiscordRPC {
    client: DiscordIpcClient,
    connected: bool,
}

impl DiscordRPC {
    // Called when starting RPC.
    pub fn new() -> Self {
        let mut client = DiscordIpcClient::new("1429782514909843506");

        let connected = client.connect().is_ok();

        if connected{
            println!("[RPC] Connected to Discord.")
        }

        Self {
            client,
            connected,
        }
    }

    // Called when updating presence.
    pub fn presence_update(&mut self, song_name: &str, artist: &str, cover_url: Option<&str>) {
        if !self.connected {
            return;
        }

        // Use album art from the grabbed URL.
        let assets = if let Some(cover_url) = cover_url {
            activity::Assets::new()
                .large_image(cover_url)
                .small_image("music")
        } else {
            activity::Assets::new().small_image("music")
        };

        let activity = activity::Activity::new()
            .state(artist)
            .details(song_name)
            .activity_type(activity::ActivityType::Listening)
            .assets(assets);

        let _ = self.client.set_activity(activity);
    }

    // Clears Presence.
    pub fn clear_presence(&mut self) {
        if !self.connected {
            return;
        }

        let _ = self.client.clear_activity();
    }
}

// Accessible outside of rpc.rs -> Calls presence_update anyway.
pub fn update_discord_presence(title: &str, artist: &str, cover_url: Option<&str>) {
    let mut rpc_guard = DISCORD_RPC.lock().unwrap();
    
    if rpc_guard.is_none() {
        *rpc_guard = Some(DiscordRPC::new());
    }
    
    if let Some(rpc) = rpc_guard.as_mut() {
        rpc.presence_update(title, artist, cover_url);
    }
}

// Same as above, just for clear.
pub fn clear_discord_presence() {
    let mut rpc_guard = DISCORD_RPC.lock().unwrap();
    
    if rpc_guard.is_none() {
        *rpc_guard = Some(DiscordRPC::new());
    }
    
    if let Some(rpc) = rpc_guard.as_mut() {
        rpc.clear_presence();
    }
}