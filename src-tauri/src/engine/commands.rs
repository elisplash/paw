// Paw Agent Engine — Channel Bridge Commands
// These are the Tauri invoke() targets for the 10 channel bridges.
// All other commands (config, trade, task, tts, state) have been moved
// to the commands:: modules layer.
//
// No imports needed here — all functions use fully-qualified crate:: paths
// or accept tauri::AppHandle directly.

// ── Telegram Bridge Commands ───────────────────────────────────────────

#[tauri::command]
pub async fn engine_telegram_start(
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    crate::engine::telegram::start_bridge(app_handle)
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
    crate::engine::telegram::load_telegram_config(&app_handle)
}

#[tauri::command]
pub fn engine_telegram_set_config(
    app_handle: tauri::AppHandle,
    config: crate::engine::telegram::TelegramConfig,
) -> Result<(), String> {
    crate::engine::telegram::save_telegram_config(&app_handle, &config)
}

#[tauri::command]
pub async fn engine_telegram_approve_user(
    app_handle: tauri::AppHandle,
    user_id: i64,
) -> Result<(), String> {
    crate::engine::telegram::approve_user(&app_handle, user_id).await
}

#[tauri::command]
pub async fn engine_telegram_deny_user(
    app_handle: tauri::AppHandle,
    user_id: i64,
) -> Result<(), String> {
    crate::engine::telegram::deny_user(&app_handle, user_id).await
}

#[tauri::command]
pub fn engine_telegram_remove_user(
    app_handle: tauri::AppHandle,
    user_id: i64,
) -> Result<(), String> {
    crate::engine::telegram::remove_user(&app_handle, user_id)
}

// ── Discord Bridge Commands ────────────────────────────────────────────

#[tauri::command]
pub async fn engine_discord_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::discord::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_discord_stop() -> Result<(), String> {
    crate::engine::discord::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_discord_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::discord::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_discord_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::discord::DiscordConfig, String> {
    crate::engine::discord::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_discord_set_config(app_handle: tauri::AppHandle, config: crate::engine::discord::DiscordConfig) -> Result<(), String> {
    crate::engine::discord::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_discord_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::discord::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_discord_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::discord::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_discord_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::discord::remove_user(&app_handle, &user_id)
}

// ── IRC Bridge Commands ────────────────────────────────────────────────

#[tauri::command]
pub async fn engine_irc_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::irc::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_irc_stop() -> Result<(), String> {
    crate::engine::irc::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_irc_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::irc::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_irc_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::irc::IrcConfig, String> {
    crate::engine::irc::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_irc_set_config(app_handle: tauri::AppHandle, config: crate::engine::irc::IrcConfig) -> Result<(), String> {
    crate::engine::irc::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_irc_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::irc::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_irc_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::irc::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_irc_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::irc::remove_user(&app_handle, &user_id)
}

// ── Slack Bridge Commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn engine_slack_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::slack::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_slack_stop() -> Result<(), String> {
    crate::engine::slack::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_slack_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::slack::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_slack_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::slack::SlackConfig, String> {
    crate::engine::slack::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_slack_set_config(app_handle: tauri::AppHandle, config: crate::engine::slack::SlackConfig) -> Result<(), String> {
    crate::engine::slack::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_slack_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::slack::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_slack_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::slack::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_slack_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::slack::remove_user(&app_handle, &user_id)
}

// ── Matrix Bridge Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn engine_matrix_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::matrix::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_matrix_stop() -> Result<(), String> {
    crate::engine::matrix::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_matrix_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::matrix::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_matrix_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::matrix::MatrixConfig, String> {
    crate::engine::matrix::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_matrix_set_config(app_handle: tauri::AppHandle, config: crate::engine::matrix::MatrixConfig) -> Result<(), String> {
    crate::engine::matrix::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_matrix_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::matrix::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_matrix_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::matrix::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_matrix_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::matrix::remove_user(&app_handle, &user_id)
}

// ── Mattermost Bridge Commands ─────────────────────────────────────────

#[tauri::command]
pub async fn engine_mattermost_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::mattermost::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_mattermost_stop() -> Result<(), String> {
    crate::engine::mattermost::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_mattermost_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::mattermost::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_mattermost_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::mattermost::MattermostConfig, String> {
    crate::engine::mattermost::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_mattermost_set_config(app_handle: tauri::AppHandle, config: crate::engine::mattermost::MattermostConfig) -> Result<(), String> {
    crate::engine::mattermost::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_mattermost_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::mattermost::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_mattermost_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::mattermost::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_mattermost_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::mattermost::remove_user(&app_handle, &user_id)
}

// ── Nextcloud Talk Bridge Commands ─────────────────────────────────────

#[tauri::command]
pub async fn engine_nextcloud_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::nextcloud::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_nextcloud_stop() -> Result<(), String> {
    crate::engine::nextcloud::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_nextcloud_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::nextcloud::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_nextcloud_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::nextcloud::NextcloudConfig, String> {
    crate::engine::nextcloud::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_nextcloud_set_config(app_handle: tauri::AppHandle, config: crate::engine::nextcloud::NextcloudConfig) -> Result<(), String> {
    crate::engine::nextcloud::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_nextcloud_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nextcloud::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_nextcloud_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nextcloud::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_nextcloud_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nextcloud::remove_user(&app_handle, &user_id)
}

// ── Nostr Bridge Commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn engine_nostr_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::nostr::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_nostr_stop() -> Result<(), String> {
    crate::engine::nostr::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_nostr_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::nostr::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_nostr_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::nostr::NostrConfig, String> {
    crate::engine::nostr::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_nostr_set_config(app_handle: tauri::AppHandle, config: crate::engine::nostr::NostrConfig) -> Result<(), String> {
    crate::engine::nostr::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_nostr_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nostr::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_nostr_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nostr::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_nostr_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nostr::remove_user(&app_handle, &user_id)
}

// ── Twitch Bridge Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn engine_twitch_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::twitch::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_twitch_stop() -> Result<(), String> {
    crate::engine::twitch::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_twitch_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::twitch::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_twitch_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::twitch::TwitchConfig, String> {
    crate::engine::twitch::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_twitch_set_config(app_handle: tauri::AppHandle, config: crate::engine::twitch::TwitchConfig) -> Result<(), String> {
    crate::engine::twitch::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_twitch_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::twitch::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_twitch_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::twitch::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_twitch_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::twitch::remove_user(&app_handle, &user_id)
}

// ── Web Chat Bridge Commands ───────────────────────────────────────────

#[tauri::command]
pub fn engine_webchat_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::webchat::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_webchat_stop() -> Result<(), String> {
    crate::engine::webchat::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_webchat_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::webchat::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_webchat_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::webchat::WebChatConfig, String> {
    crate::engine::webchat::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_webchat_set_config(app_handle: tauri::AppHandle, config: crate::engine::webchat::WebChatConfig) -> Result<(), String> {
    crate::engine::webchat::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_webchat_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::webchat::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_webchat_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::webchat::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_webchat_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::webchat::remove_user(&app_handle, &user_id)
}
