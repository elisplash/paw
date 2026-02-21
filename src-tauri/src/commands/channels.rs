// commands/channels.rs — Channel bridge command generation via macro.
//
// Uses `paste::paste!` to generate all 8 Tauri commands per bridge from a
// single declarative macro invocation, eliminating 360+ lines of boilerplate.
//
// ── API contract for "standard" channels ─────────────────────────────────────
//   start_bridge(app_handle: AppHandle) -> Result<(), String>   (sync, called from async cmd)
//   stop_bridge()                                               (no args)
//   get_status(&app_handle)     -> ChannelStatus
//   load_config(&app_handle)    -> Result<Config>
//   save_config(&app_handle, &config) -> Result<()>
//   approve_user(&app_handle, &str) -> Result<()>
//   deny_user(&app_handle, &str)    -> Result<()>
//   remove_user(&app_handle, &str)  -> Result<()>
//
// Standard channels (9): discord, irc, slack, matrix, mattermost,
//                        nextcloud, nostr, twitch, webchat
//
// ── Telegram is hand-written (unique API) ────────────────────────────────────
//   - Status type:  TelegramStatus  (not ChannelStatus)
//   - Config fns:   load_telegram_config / save_telegram_config
//   - User ID type: i64             (not String)
//   - approve/deny: async + .await  (not sync)
// ─────────────────────────────────────────────────────────────────────────────

/// Generate all 8 Tauri commands for a standard channel bridge.
/// `$name` becomes the channel segment in every function name (e.g. `discord`).
/// `$module` is the fully-qualified engine module path.
/// `$config_type` is the channel's config struct type.
macro_rules! channel_commands {
    ($name:ident, $module:path, $config_type:ty) => {
        paste::paste! {
            /// Start the `$name` bridge.
            #[tauri::command]
            pub async fn [<engine_ $name _start>](
                app_handle: tauri::AppHandle,
            ) -> Result<(), String> {
                $module::start_bridge(app_handle).map_err(|e| e.to_string())
            }

            /// Stop the `$name` bridge.
            #[tauri::command]
            pub fn [<engine_ $name _stop>]() -> Result<(), String> {
                $module::stop_bridge();
                Ok(())
            }

            /// Get the current `$name` bridge status.
            #[tauri::command]
            pub fn [<engine_ $name _status>](
                app_handle: tauri::AppHandle,
            ) -> Result<crate::engine::channels::ChannelStatus, String> {
                Ok($module::get_status(&app_handle))
            }

            /// Read the `$name` bridge configuration.
            #[tauri::command]
            pub fn [<engine_ $name _get_config>](
                app_handle: tauri::AppHandle,
            ) -> Result<$config_type, String> {
                $module::load_config(&app_handle).map_err(|e| e.to_string())
            }

            /// Persist new `$name` bridge configuration.
            #[tauri::command]
            pub fn [<engine_ $name _set_config>](
                app_handle: tauri::AppHandle,
                config: $config_type,
            ) -> Result<(), String> {
                $module::save_config(&app_handle, &config).map_err(|e| e.to_string())
            }

            /// Allow a user to interact with this agent via `$name`.
            #[tauri::command]
            pub fn [<engine_ $name _approve_user>](
                app_handle: tauri::AppHandle,
                user_id: String,
            ) -> Result<(), String> {
                $module::approve_user(&app_handle, &user_id).map_err(|e| e.to_string())
            }

            /// Block a user from interacting via `$name`.
            #[tauri::command]
            pub fn [<engine_ $name _deny_user>](
                app_handle: tauri::AppHandle,
                user_id: String,
            ) -> Result<(), String> {
                $module::deny_user(&app_handle, &user_id).map_err(|e| e.to_string())
            }

            /// Remove a user's stored record from the `$name` allowlist.
            #[tauri::command]
            pub fn [<engine_ $name _remove_user>](
                app_handle: tauri::AppHandle,
                user_id: String,
            ) -> Result<(), String> {
                $module::remove_user(&app_handle, &user_id).map_err(|e| e.to_string())
            }
        }
    };
}

// ── Generate commands for the 9 standard channels ────────────────────────────

channel_commands!(
    discord,
    crate::engine::discord,
    crate::engine::discord::DiscordConfig
);
channel_commands!(
    irc,
    crate::engine::irc,
    crate::engine::irc::IrcConfig
);
channel_commands!(
    slack,
    crate::engine::slack,
    crate::engine::slack::SlackConfig
);
channel_commands!(
    matrix,
    crate::engine::matrix,
    crate::engine::matrix::MatrixConfig
);
channel_commands!(
    mattermost,
    crate::engine::mattermost,
    crate::engine::mattermost::MattermostConfig
);
channel_commands!(
    nextcloud,
    crate::engine::nextcloud,
    crate::engine::nextcloud::NextcloudConfig
);
channel_commands!(
    nostr,
    crate::engine::nostr,
    crate::engine::nostr::NostrConfig
);
channel_commands!(
    twitch,
    crate::engine::twitch,
    crate::engine::twitch::TwitchConfig
);
channel_commands!(
    webchat,
    crate::engine::webchat,
    crate::engine::webchat::WebChatConfig
);
channel_commands!(
    whatsapp,
    crate::engine::whatsapp,
    crate::engine::whatsapp::WhatsAppConfig
);

// ── Telegram — hand-written (unique API surface) ──────────────────────────────
//
// Differences from standard channels:
//   • Status type is TelegramStatus, not ChannelStatus
//   • Config helpers named load_telegram_config / save_telegram_config
//   • User IDs are i64 (Telegram user IDs are numeric)
//   • approve_user and deny_user are async (they send bot messages)
//   • remove_user is sync

#[tauri::command]
pub async fn engine_telegram_start(
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    crate::engine::telegram::start_bridge(app_handle).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_telegram_stop() -> Result<(), String> {
    crate::engine::telegram::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_telegram_status(
    app_handle: tauri::AppHandle,
) -> Result<crate::engine::telegram::TelegramStatus, String> {
    Ok(crate::engine::telegram::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_telegram_get_config(
    app_handle: tauri::AppHandle,
) -> Result<crate::engine::telegram::TelegramConfig, String> {
    crate::engine::telegram::load_telegram_config(&app_handle).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_telegram_set_config(
    app_handle: tauri::AppHandle,
    config: crate::engine::telegram::TelegramConfig,
) -> Result<(), String> {
    crate::engine::telegram::save_telegram_config(&app_handle, &config).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn engine_telegram_approve_user(
    app_handle: tauri::AppHandle,
    user_id: i64,
) -> Result<(), String> {
    crate::engine::telegram::approve_user(&app_handle, user_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn engine_telegram_deny_user(
    app_handle: tauri::AppHandle,
    user_id: i64,
) -> Result<(), String> {
    crate::engine::telegram::deny_user(&app_handle, user_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_telegram_remove_user(
    app_handle: tauri::AppHandle,
    user_id: i64,
) -> Result<(), String> {
    crate::engine::telegram::remove_user(&app_handle, user_id).map_err(|e| e.to_string())
}

// ── NOTE on tauri::generate_handler! ─────────────────────────────────────────
// generate_handler! is a *proc-macro*, not macro_rules!, so inner macro
// invocations are NOT eagerly expanded inside it.  The 80 handler paths are
// therefore listed explicitly in lib.rs under `commands::channels::*`.
