// Native client modules (only when native-client feature enabled)
#[cfg(feature = "native-client")]
pub mod native;

// Tauri app modules (only when tauri-app feature enabled)
#[cfg(feature = "tauri-app")]
mod auth;
#[cfg(feature = "tauri-app")]
mod api;
#[cfg(feature = "tauri-app")]
mod games;
#[cfg(feature = "tauri-app")]
mod streaming;
#[cfg(feature = "tauri-app")]
mod config;
#[cfg(feature = "tauri-app")]
mod discord;
#[cfg(feature = "tauri-app")]
mod proxy;
#[cfg(feature = "tauri-app")]
mod cursor;

#[cfg(feature = "tauri-app")]
use tauri::Manager;

#[cfg(feature = "tauri-app")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|_app| {
            #[cfg(debug_assertions)]
            {
                if let Some(window) = _app.get_webview_window("main") {
                    let _ = window.open_devtools();
                }
            }

            // Initialize Discord Rich Presence in background
            tauri::async_runtime::spawn(async {
                if let Err(e) = discord::init_discord().await {
                    log::warn!("Failed to initialize Discord: {}", e);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Auth commands
            auth::login,
            auth::login_oauth,
            auth::set_access_token,
            auth::get_access_token,
            auth::get_gfn_jwt,
            auth::logout,
            auth::get_auth_status,
            auth::refresh_token,
            // API commands
            api::fetch_games,
            api::fetch_library,
            api::fetch_main_games,
            api::search_games,
            api::search_games_graphql,
            api::get_game_details,
            api::get_servers,
            api::fetch_subscription,
            // Streaming commands
            streaming::start_session,
            streaming::stop_session,
            streaming::poll_session_until_ready,
            streaming::cancel_polling,
            streaming::is_polling_active,
            streaming::get_webrtc_config,
            streaming::start_streaming_flow,
            streaming::stop_streaming_flow,
            // Session detection commands
            streaming::get_active_sessions,
            streaming::terminate_session,
            streaming::setup_reconnect_session,
            streaming::claim_session,
            // Config commands
            config::get_settings,
            config::save_settings,
            // Discord commands
            discord::init_discord,
            discord::set_game_presence,
            discord::update_game_stats,
            discord::set_queue_presence,
            discord::set_browsing_presence,
            discord::clear_discord_presence,
            discord::disconnect_discord,
            discord::is_discord_connected,
            // Proxy commands
            proxy::get_proxy_settings,
            proxy::set_proxy_settings,
            proxy::enable_proxy,
            proxy::disable_proxy,
            proxy::test_proxy,
            // Cursor capture commands (macOS and Windows native)
            cursor::capture_cursor,
            cursor::release_cursor,
            cursor::is_cursor_captured,
            cursor::get_mouse_delta,
            cursor::recenter_cursor,
            // High-frequency mouse polling (Windows)
            cursor::start_mouse_polling,
            cursor::stop_mouse_polling,
            cursor::get_accumulated_mouse_delta,
            cursor::is_mouse_polling_active,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
