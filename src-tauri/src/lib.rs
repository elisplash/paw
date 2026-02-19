// Paw — Tauri application entry point.
// All helper functions (mail, keychain, weather) have been moved to commands:: modules.
// This file now contains only the module declarations and the Tauri app builder.

// ── Paw Atoms (constants, error types) ────────────────────────────────────
pub mod atoms;

// ── Paw Agent Engine ───────────────────────────────────────────────────
pub mod engine;

// ── Paw Command Modules (Systems layer) ───────────────────────────────
pub mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize the Paw Agent Engine state
    let engine_state = commands::state::EngineState::new()
        .expect("Failed to initialize Paw Agent Engine");

    tauri::Builder::default()
        .manage(engine_state)
        .plugin(tauri_plugin_log::Builder::new()
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::LogDir { file_name: Some("openpawz".into()) },
            ))
            .max_file_size(5_000_000) // 5MB max per log file
            .build())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // ── Cron Heartbeat: autonomous task execution ──
            // Spawns a background loop that checks for due cron tasks
            // every 60 seconds and auto-executes them.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Initial delay: let the app fully initialize
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                log::info!("[heartbeat] Cron heartbeat started (60s interval)");

                loop {
                    commands::task::run_cron_heartbeat(&app_handle).await;
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // ── Mail (himalaya bridge) ──
            commands::mail::write_himalaya_config,
            commands::mail::read_himalaya_config,
            commands::mail::remove_himalaya_account,
            commands::mail::fetch_emails,
            commands::mail::fetch_email_content,
            commands::mail::send_email,
            commands::mail::list_mail_folders,
            commands::mail::move_email,
            commands::mail::delete_email,
            commands::mail::set_email_flag,
            // ── Utility (keychain, weather, db crypto) ──
            commands::utility::keyring_has_password,
            commands::utility::keyring_delete_password,
            commands::utility::fetch_weather,
            commands::utility::get_db_encryption_key,
            commands::utility::has_db_encryption_key,
            // ── Chat & Sessions ──
            commands::chat::engine_chat_send,
            commands::chat::engine_chat_history,
            commands::chat::engine_sessions_list,
            commands::chat::engine_session_rename,
            commands::chat::engine_session_delete,
            commands::chat::engine_session_clear,
            commands::chat::engine_session_compact,
            commands::chat::engine_approve_tool,
            // ── Engine Config & Sandbox ──
            commands::config::engine_sandbox_check,
            commands::config::engine_sandbox_get_config,
            commands::config::engine_sandbox_set_config,
            commands::config::engine_get_config,
            commands::config::engine_get_daily_spend,
            commands::config::engine_set_config,
            commands::config::engine_upsert_provider,
            commands::config::engine_remove_provider,
            commands::config::engine_status,
            commands::config::engine_auto_setup,
            // ── Agent Files (Soul / Persona) ──
            commands::agent::engine_agent_file_list,
            commands::agent::engine_agent_file_get,
            commands::agent::engine_agent_file_set,
            commands::agent::engine_agent_file_delete,
            commands::agent::engine_list_all_agents,
            commands::agent::engine_create_agent,
            commands::agent::engine_delete_agent,
            // ── Memory (Long-term Semantic) ──
            commands::memory::engine_memory_store,
            commands::memory::engine_memory_search,
            commands::memory::engine_memory_stats,
            commands::memory::engine_memory_delete,
            commands::memory::engine_memory_list,
            commands::memory::engine_get_memory_config,
            commands::memory::engine_set_memory_config,
            commands::memory::engine_test_embedding,
            commands::memory::engine_embedding_status,
            commands::memory::engine_embedding_pull_model,
            commands::memory::engine_ensure_embedding_ready,
            commands::memory::engine_memory_backfill,
            // ── Skill Vault ──
            commands::skills::engine_skills_list,
            commands::skills::engine_skill_set_enabled,
            commands::skills::engine_skill_set_credential,
            commands::skills::engine_skill_delete_credential,
            commands::skills::engine_skill_revoke_all,
            commands::skills::engine_skill_get_instructions,
            commands::skills::engine_skill_set_instructions,
            // ── Trading ──
            commands::trade::engine_trading_history,
            commands::trade::engine_trading_summary,
            commands::trade::engine_trading_policy_get,
            commands::trade::engine_trading_policy_set,
            // ── Positions (Stop-Loss / Take-Profit) ──
            commands::trade::engine_positions_list,
            commands::trade::engine_position_close,
            commands::trade::engine_position_update_targets,
            // ── Text-to-Speech ──
            commands::tts::engine_tts_speak,
            commands::tts::engine_tts_get_config,
            commands::tts::engine_tts_set_config,
            // ── Tasks (Kanban Board) ──
            commands::task::engine_tasks_list,
            commands::task::engine_task_create,
            commands::task::engine_task_update,
            commands::task::engine_task_delete,
            commands::task::engine_task_move,
            commands::task::engine_task_activity,
            commands::task::engine_task_set_agents,
            commands::task::engine_task_run,
            commands::task::engine_tasks_cron_tick,
            // ── Telegram Bridge ──
            engine::commands::engine_telegram_start,
            engine::commands::engine_telegram_stop,
            engine::commands::engine_telegram_status,
            engine::commands::engine_telegram_get_config,
            engine::commands::engine_telegram_set_config,
            engine::commands::engine_telegram_approve_user,
            engine::commands::engine_telegram_deny_user,
            engine::commands::engine_telegram_remove_user,
            // ── Discord Bridge ──
            engine::commands::engine_discord_start,
            engine::commands::engine_discord_stop,
            engine::commands::engine_discord_status,
            engine::commands::engine_discord_get_config,
            engine::commands::engine_discord_set_config,
            engine::commands::engine_discord_approve_user,
            engine::commands::engine_discord_deny_user,
            engine::commands::engine_discord_remove_user,
            // ── IRC Bridge ──
            engine::commands::engine_irc_start,
            engine::commands::engine_irc_stop,
            engine::commands::engine_irc_status,
            engine::commands::engine_irc_get_config,
            engine::commands::engine_irc_set_config,
            engine::commands::engine_irc_approve_user,
            engine::commands::engine_irc_deny_user,
            engine::commands::engine_irc_remove_user,
            // ── Slack Bridge ──
            engine::commands::engine_slack_start,
            engine::commands::engine_slack_stop,
            engine::commands::engine_slack_status,
            engine::commands::engine_slack_get_config,
            engine::commands::engine_slack_set_config,
            engine::commands::engine_slack_approve_user,
            engine::commands::engine_slack_deny_user,
            engine::commands::engine_slack_remove_user,
            // ── Matrix Bridge ──
            engine::commands::engine_matrix_start,
            engine::commands::engine_matrix_stop,
            engine::commands::engine_matrix_status,
            engine::commands::engine_matrix_get_config,
            engine::commands::engine_matrix_set_config,
            engine::commands::engine_matrix_approve_user,
            engine::commands::engine_matrix_deny_user,
            engine::commands::engine_matrix_remove_user,
            // ── Mattermost Bridge ──
            engine::commands::engine_mattermost_start,
            engine::commands::engine_mattermost_stop,
            engine::commands::engine_mattermost_status,
            engine::commands::engine_mattermost_get_config,
            engine::commands::engine_mattermost_set_config,
            engine::commands::engine_mattermost_approve_user,
            engine::commands::engine_mattermost_deny_user,
            engine::commands::engine_mattermost_remove_user,
            // ── Nextcloud Talk Bridge ──
            engine::commands::engine_nextcloud_start,
            engine::commands::engine_nextcloud_stop,
            engine::commands::engine_nextcloud_status,
            engine::commands::engine_nextcloud_get_config,
            engine::commands::engine_nextcloud_set_config,
            engine::commands::engine_nextcloud_approve_user,
            engine::commands::engine_nextcloud_deny_user,
            engine::commands::engine_nextcloud_remove_user,
            // ── Nostr Bridge ──
            engine::commands::engine_nostr_start,
            engine::commands::engine_nostr_stop,
            engine::commands::engine_nostr_status,
            engine::commands::engine_nostr_get_config,
            engine::commands::engine_nostr_set_config,
            engine::commands::engine_nostr_approve_user,
            engine::commands::engine_nostr_deny_user,
            engine::commands::engine_nostr_remove_user,
            // ── Twitch Bridge ──
            engine::commands::engine_twitch_start,
            engine::commands::engine_twitch_stop,
            engine::commands::engine_twitch_status,
            engine::commands::engine_twitch_get_config,
            engine::commands::engine_twitch_set_config,
            engine::commands::engine_twitch_approve_user,
            engine::commands::engine_twitch_deny_user,
            engine::commands::engine_twitch_remove_user,
            // ── Web Chat Bridge ──
            engine::commands::engine_webchat_start,
            engine::commands::engine_webchat_stop,
            engine::commands::engine_webchat_status,
            engine::commands::engine_webchat_get_config,
            engine::commands::engine_webchat_set_config,
            engine::commands::engine_webchat_approve_user,
            engine::commands::engine_webchat_deny_user,
            engine::commands::engine_webchat_remove_user,
            // ── Orchestrator: Projects ──
            commands::project::engine_projects_list,
            commands::project::engine_project_create,
            commands::project::engine_project_update,
            commands::project::engine_project_delete,
            commands::project::engine_project_set_agents,
            commands::project::engine_project_messages,
            commands::project::engine_project_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
