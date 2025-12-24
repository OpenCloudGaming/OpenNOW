use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::command;

/// Discord Application ID for OpenNOW
/// Created at https://discord.com/developers/applications
const DISCORD_APP_ID: &str = "1453497742662959145"; // Replace with your Discord App ID

/// GitHub repository URL
const GITHUB_URL: &str = "https://github.com/zortos293/GFNClient";

/// Discord presence state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceState {
    pub enabled: bool,
    pub current_game: Option<String>,
    pub details: Option<String>,
    pub start_time: Option<i64>,
}

/// Global Discord client - using std::sync::Mutex since DiscordIpcClient is not Send
static DISCORD_CLIENT: std::sync::OnceLock<Mutex<Option<DiscordIpcClient>>> =
    std::sync::OnceLock::new();

fn get_discord_client() -> &'static Mutex<Option<DiscordIpcClient>> {
    DISCORD_CLIENT.get_or_init(|| Mutex::new(None))
}

/// Initialize Discord Rich Presence
#[command]
pub async fn init_discord() -> Result<bool, String> {
    log::info!("Initializing Discord Rich Presence");

    // Run Discord connection in blocking task since it's not async
    let result = tokio::task::spawn_blocking(|| {
        let mut client = DiscordIpcClient::new(DISCORD_APP_ID)
            .map_err(|e| format!("Failed to create Discord client: {}", e))?;

        match client.connect() {
            Ok(_) => {
                log::info!("Discord Rich Presence connected");

                // Set initial presence
                let _ = client.set_activity(
                    activity::Activity::new()
                        .state("GeForce NOW via OpenNOW")
                        .details("Browsing games")
                        .assets(
                            activity::Assets::new()
                                .large_image("gfn_logo")
                                .large_text("OpenNOW"),
                        )
                        .buttons(vec![
                            activity::Button::new("GitHub", GITHUB_URL),
                        ]),
                );

                let guard = get_discord_client();
                let mut lock = guard.lock().map_err(|e| format!("Lock error: {}", e))?;
                *lock = Some(client);

                Ok(true)
            }
            Err(e) => {
                log::warn!("Discord not available: {}", e);
                Ok(false)
            }
        }
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?;

    result
}

/// Update Discord presence when playing a game
#[command]
pub async fn set_game_presence(
    game_name: String,
    region: Option<String>,
    resolution: Option<String>,
    fps: Option<u32>,
    latency_ms: Option<u32>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let guard = get_discord_client();
        let mut lock = guard.lock().map_err(|e| format!("Lock error: {}", e))?;

        if let Some(client) = lock.as_mut() {
            let start_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            // Build stats string: "1080p 120fps 15ms"
            let mut stats_parts: Vec<String> = Vec::new();
            if let Some(res) = &resolution {
                stats_parts.push(res.clone());
            }
            if let Some(f) = fps {
                stats_parts.push(format!("{}fps", f));
            }
            if let Some(ms) = latency_ms {
                stats_parts.push(format!("{}ms", ms));
            }

            // State shows region + stats
            let state = if let Some(reg) = &region {
                if stats_parts.is_empty() {
                    reg.clone()
                } else {
                    format!("{} | {}", reg, stats_parts.join(" "))
                }
            } else if !stats_parts.is_empty() {
                stats_parts.join(" ")
            } else {
                "GeForce NOW via OpenNOW".to_string()
            };

            let activity = activity::Activity::new()
                .state(&state)
                .details(&game_name)
                .assets(
                    activity::Assets::new()
                        .large_image("gfn_logo")
                        .large_text("OpenNOW")
                        .small_image("playing")
                        .small_text("Playing"),
                )
                .timestamps(activity::Timestamps::new().start(start_time))
                .buttons(vec![
                    activity::Button::new("GitHub", GITHUB_URL),
                ]);

            client
                .set_activity(activity)
                .map_err(|e| format!("Failed to set presence: {}", e))?;

            log::info!("Discord presence updated: playing {} ({})", game_name, state);
        }

        Ok(())
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

/// Update Discord presence with streaming stats (call periodically during gameplay)
#[command]
pub async fn update_game_stats(
    game_name: String,
    region: Option<String>,
    resolution: Option<String>,
    fps: Option<u32>,
    latency_ms: Option<u32>,
    start_time: Option<i64>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let guard = get_discord_client();
        let mut lock = guard.lock().map_err(|e| format!("Lock error: {}", e))?;

        if let Some(client) = lock.as_mut() {
            // Build stats string
            let mut stats_parts: Vec<String> = Vec::new();
            if let Some(res) = &resolution {
                stats_parts.push(res.clone());
            }
            if let Some(f) = fps {
                stats_parts.push(format!("{}fps", f));
            }
            if let Some(ms) = latency_ms {
                stats_parts.push(format!("{}ms", ms));
            }

            let state = if let Some(reg) = &region {
                if stats_parts.is_empty() {
                    reg.clone()
                } else {
                    format!("{} | {}", reg, stats_parts.join(" "))
                }
            } else if !stats_parts.is_empty() {
                stats_parts.join(" ")
            } else {
                "GeForce NOW via OpenNOW".to_string()
            };

            // Use provided start_time to preserve elapsed time
            let timestamp = start_time.unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64
            });

            let activity = activity::Activity::new()
                .state(&state)
                .details(&game_name)
                .assets(
                    activity::Assets::new()
                        .large_image("gfn_logo")
                        .large_text("OpenNOW")
                        .small_image("playing")
                        .small_text("Playing"),
                )
                .timestamps(activity::Timestamps::new().start(timestamp))
                .buttons(vec![
                    activity::Button::new("GitHub", GITHUB_URL),
                ]);

            client
                .set_activity(activity)
                .map_err(|e| format!("Failed to set presence: {}", e))?;
        }

        Ok(())
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

/// Update Discord presence when in queue
#[command]
pub async fn set_queue_presence(
    game_name: String,
    queue_position: Option<u32>,
    eta_seconds: Option<u32>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let guard = get_discord_client();
        let mut lock = guard.lock().map_err(|e| format!("Lock error: {}", e))?;

        if let Some(client) = lock.as_mut() {
            let state = match queue_position {
                Some(pos) => format!("In queue: #{}", pos),
                None => "Waiting in queue".to_string(),
            };

            let details = format!("Waiting to play {}", game_name);
            let mut activity = activity::Activity::new()
                .state(&state)
                .details(&details)
                .assets(
                    activity::Assets::new()
                        .large_image("gfn_logo")
                        .large_text("OpenNOW")
                        .small_image("queue")
                        .small_text("In Queue"),
                )
                .buttons(vec![
                    activity::Button::new("GitHub", GITHUB_URL),
                ]);

            // Add ETA if available
            if let Some(eta) = eta_seconds {
                let end_time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64
                    + eta as i64;
                activity = activity.timestamps(activity::Timestamps::new().end(end_time));
            }

            client
                .set_activity(activity)
                .map_err(|e| format!("Failed to set presence: {}", e))?;

            log::info!("Discord presence updated: in queue for {}", game_name);
        }

        Ok(())
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

/// Update Discord presence when browsing
#[command]
pub async fn set_browsing_presence() -> Result<(), String> {
    tokio::task::spawn_blocking(|| {
        let guard = get_discord_client();
        let mut lock = guard.lock().map_err(|e| format!("Lock error: {}", e))?;

        if let Some(client) = lock.as_mut() {
            client
                .set_activity(
                    activity::Activity::new()
                        .state("GeForce NOW via OpenNOW")
                        .details("Browsing games")
                        .assets(
                            activity::Assets::new()
                                .large_image("gfn_logo")
                                .large_text("OpenNOW"),
                        )
                        .buttons(vec![
                            activity::Button::new("GitHub", GITHUB_URL),
                        ]),
                )
                .map_err(|e| format!("Failed to set presence: {}", e))?;

            log::info!("Discord presence updated: browsing");
        }

        Ok(())
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

/// Clear Discord presence
#[command]
pub async fn clear_discord_presence() -> Result<(), String> {
    tokio::task::spawn_blocking(|| {
        let guard = get_discord_client();
        let mut lock = guard.lock().map_err(|e| format!("Lock error: {}", e))?;

        if let Some(client) = lock.as_mut() {
            client
                .clear_activity()
                .map_err(|e| format!("Failed to clear presence: {}", e))?;

            log::info!("Discord presence cleared");
        }

        Ok(())
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

/// Disconnect from Discord
#[command]
pub async fn disconnect_discord() -> Result<(), String> {
    tokio::task::spawn_blocking(|| {
        let guard = get_discord_client();
        let mut lock = guard.lock().map_err(|e| format!("Lock error: {}", e))?;

        if let Some(mut client) = lock.take() {
            let _ = client.close();
            log::info!("Discord Rich Presence disconnected");
        }

        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

/// Check if Discord is connected
#[command]
pub async fn is_discord_connected() -> bool {
    let guard = get_discord_client();
    let lock = match guard.lock() {
        Ok(l) => l,
        Err(_) => return false,
    };
    lock.is_some()
}
