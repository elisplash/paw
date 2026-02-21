# Engine Refactor — Phase Plan

> Six god-files in `src-tauri/src/engine/` violate the atomic architecture.  
> This document is a phase-by-phase plan for splitting them into focused modules.  
> Follow the same pattern established by the `tools/` refactor (see `TOOLS_REFACTOR.md`).

---

## Problem

| File | Lines | What's in it |
|------|-------|-------------|
| `dex.rs` | 2,822 | 14 EVM DEX tool executors + ABI encoder + RLP + EIP-1559 signing + JSON-RPC client + token registry + DexScreener API |
| `sessions.rs` | 1,949 | Schema migrations + session CRUD + message CRUD + config store + trades + positions + agent files + memories + tasks + projects + orchestrator bus + vector math |
| `skills.rs` | 1,700 | 40 skill definitions + credential vault (SQLite + XOR encryption) + skill status + prompt injection + community skills (SKILL.md parser, GitHub fetcher, skills.sh search, DB CRUD) |
| `sol_dex.rs` | 1,582 | 7 Solana DEX tool executors + Jupiter + PumpPortal + Solana RPC client + ed25519 signing + transaction builder + token registry |
| `whatsapp.rs` | 1,223 | Docker lifecycle + Colima discovery + Evolution API container management + webhook HTTP server + message handling + bridge orchestration |
| `telegram.rs` | 931 | Bot API types + HTTP transport + bridge lifecycle + 329-line `run_telegram_agent()` (duplicates `channels::run_channel_agent`) + config persistence |

**Total: 10,207 lines across 6 files.**

---

## Concept Clarification — Skills vs Integrations vs Extensions vs Tools

The codebase conflates four concepts the docs describe as distinct tiers:

```
┌──────────────┬────────────────────────────────────────────────────────────┐
│ Concept      │ Role                                                      │
├──────────────┼────────────────────────────────────────────────────────────┤
│ Tool         │ An LLM-callable function (ToolDefinition).                │
│              │ Lives in src-tauri/src/engine/tools/.                     │
│              │ Already refactored — 17 focused modules.                  │
├──────────────┼────────────────────────────────────────────────────────────┤
│ Skill        │ A prompt-only instruction pack (SKILL.md).                │
│              │ Community-installable from skills.sh.                     │
│              │ No credentials, no native tools, no binaries.             │
│              │ Tier 1 in the extensibility system.                       │
├──────────────┼────────────────────────────────────────────────────────────┤
│ Integration  │ A credential-bearing configuration bundle.                │
│              │ Has: encrypted vault credentials, optional tool gating,   │
│              │ optional binary requirements, agent instructions.         │
│              │ Includes all current "Vault" + "API" + "CLI" builtins.    │
│              │ Tier 2 (TOML-based community integrations are planned).   │
├──────────────┼────────────────────────────────────────────────────────────┤
│ Extension    │ An integration + custom sidebar view + persistent         │
│              │ storage. Tier 3 — not yet built.                          │
└──────────────┴────────────────────────────────────────────────────────────┘
```

**Current confusion in code:**
- `builtin_skills()` in `skills.rs` defines 40 items. Most are **integrations** (have credentials, enable tools). The code calls everything a "skill."
- `tools/integrations.rs` is just `rest_api_call` + `webhook_send` + `image_generate` — not a general integration system.
- The docs describe TOML manifests, PawzHub, and storage tools that don't exist yet.
- `integrations.md` says AES-GCM encryption; the code uses XOR cipher.

**Action:** During the `skills.rs` refactor (Phase 2), introduce a `SkillTier` enum and separate naming. Full rename to `Integration`/`Extension` types is deferred until those systems are built.

---

## Per-File Contract

Same pattern as the `tools/` refactor. Each module in a directory:
- Owns its types, its logic, and its tests
- Has a clear single responsibility  
- Re-exports through `mod.rs`
- Uses `pub(crate)` for internal engine APIs

---

## Phase 1 — `dex.rs` → `dex/` (2,822 lines → 12 files)

The largest file. Contains a full EVM toolkit: ABI encoding, RLP serialization, EIP-1559 signing, JSON-RPC client, plus 14 tool executor functions.

### Target Structure

```
src-tauri/src/engine/dex/
├── mod.rs              // re-exports all pub items
├── constants.rs        // KNOWN_TOKENS, contract addresses, defaults, explorer URLs
├── primitives.rs       // keccak256, hex_encode/decode, address_from_pubkey, eip55_checksum,
│                       // parse_address, parse_u256_decimal, amount_to_raw, raw_to_amount
├── abi.rs              // function_selector, abi_encode_*, encode_balance_of, encode_approve,
│                       // encode_allowance, ERC-20 introspection (name/symbol/decimals/supply),
│                       // decode_abi_string, Uniswap path encoding
├── rlp.rs              // rlp_encode_bytes, rlp_encode_list, to_minimal_be_bytes,
│                       // u64_to_minimal_be, u256_to_minimal_be
├── tx.rs               // sign_eip1559_transaction
├── rpc.rs              // rpc_call, eth_get_balance, eth_call, eth_get_transaction_count,
│                       // get_gas_fees, eth_estimate_gas, eth_send_raw_transaction,
│                       // eth_chain_id, eth_get_transaction_receipt, chunked_get_logs
├── tokens.rs           // resolve_token, resolve_for_swap
├── wallet.rs           // execute_dex_wallet_create
├── swap.rs             // execute_dex_quote, execute_dex_swap
├── portfolio.rs        // execute_dex_balance, execute_dex_portfolio
├── transfer.rs         // encode_transfer, execute_dex_transfer
├── token_analysis.rs   // execute_dex_token_info, execute_dex_check_token
├── discovery.rs        // execute_dex_search_token, execute_dex_trending, urlencoding
└── monitoring.rs       // execute_dex_watch_wallet, execute_dex_whale_transfers,
                        // execute_dex_top_traders, parse_transfer_log,
                        // format_large_number, Transfer, WalletProfile, TraderScore
```

### DRY Fixes During This Phase

| Duplication | Action |
|-------------|--------|
| Chain ID → explorer URL mapped 3× inline | Extract `fn explorer_tx_url(chain_id, tx_hash) -> String` into `constants.rs` |
| Chain ID → name mapped 4× inline | Extract `fn chain_name(chain_id) -> &str` into `constants.rs` |
| Honeypot round-trip test logic duplicated between `token_info` and `check_token` | Extract shared `fn honeypot_roundtrip_test()` into `token_analysis.rs` |

### Call Sites to Rewire

The `dex.rs` public functions are called from `tools/dex.rs` (tool executor module). Update imports:
```rust
// Before
use crate::engine::dex::execute_dex_swap;
// After (same path, dex is now a directory module)
use crate::engine::dex::execute_dex_swap;  // mod.rs re-exports — no change needed if re-exports are correct
```

### Verification

```bash
cargo check 2>&1 | tail -5   # must compile clean
grep -rn "engine::dex::" src/ --include="*.rs" | grep -v "mod.rs"  # verify all call sites
```

---

## Phase 2 — `skills.rs` → `skills/` (1,700 lines → 7 files)

Skills, credential vault, encryption, community marketplace — all in one file.

### Target Structure

```
src-tauri/src/engine/skills/
├── mod.rs              // re-exports, SkillTier enum
├── types.rs            // SkillCategory, SkillDefinition, CredentialField, SkillRecord, SkillStatus
├── builtins.rs         // builtin_skills() — all 40 SkillDefinition literals
├── vault.rs            // SessionStore impl: credential CRUD, enable/disable, custom instructions
├── crypto.rs           // get_vault_key, encrypt_credential, decrypt_credential
├── status.rs           // get_all_skill_status (combines definitions + DB + binary + env)
├── prompt.rs           // get_enabled_skill_instructions, inject_credentials_into_instructions
└── community/
    ├── mod.rs          // re-exports
    ├── types.rs        // CommunitySkill, DiscoveredSkill
    ├── parser.rs       // parse_skill_md (YAML frontmatter + markdown)
    ├── github.rs       // fetch_repo_skills, install_community_skill, parse_github_source
    ├── search.rs       // search_community_skills (skills.sh API)
    └── store.rs        // SessionStore impl: community skill DB CRUD
```

### Internal Duplication to Fix

- `builtins.rs` is 764 lines of struct literals. Consider a macro or builder pattern to reduce boilerplate — but readability matters more than DRY here. Keep literals, just isolate them.

### Call Sites

`skills.rs` is imported by: `commands.rs`, `tools/skills_tools.rs`, `agent_loop.rs`, `orchestrator.rs`, `chat.rs`. All use `crate::engine::skills::*` — the `mod.rs` re-exports keep these working.

### Naming Note

The module stays named `skills/` for now. A future rename to `integrations/` (with `skills/` reserved for Tier 1 community skills only) would happen when the TOML-based integration system is built. For this refactor, add a `SkillTier` enum to `types.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillTier {
    Skill,        // Tier 1 — prompt-only SKILL.md
    Integration,  // Tier 2 — credentials + tools + binaries
    Extension,    // Tier 3 — integration + views + storage (future)
}
```

---

## Phase 3 — `sessions.rs` → `sessions/` (1,949 lines → 13 files)

The `SessionStore` is a mega-struct with impl blocks spanning every domain: sessions, messages, config, memories, trades, positions, tasks, projects. Splitting it preserves the single `SessionStore` struct but distributes its methods across focused modules.

### Target Structure

```
src-tauri/src/engine/sessions/
├── mod.rs              // SessionStore struct, engine_db_path(), open(), re-exports
├── schema.rs           // DDL strings, CREATE TABLE/INDEX, ALTER TABLE migrations
├── sessions.rs         // create_session, list_sessions, list_sessions_filtered,
│                       // get_session, rename_session, delete_session,
│                       // clear_messages, cleanup_empty_sessions, prune_session_messages
├── messages.rs         // add_message, get_messages, load_conversation, sanitize_tool_pairs
├── config.rs           // get_config, set_config
├── memories.rs         // store_memory, delete_memory, memory_stats,
│                       // search_memories_by_embedding, search_memories_bm25,
│                       // search_memories_keyword, list_memories,
│                       // list_memories_without_embeddings, update_memory_embedding,
│                       // get_todays_memories
├── agent_files.rs      // list_agent_files, get_agent_file, set_agent_file,
│                       // delete_agent_file, compose_agent_context, compose_core_context
├── tasks.rs            // list_tasks, create_task, update_task, delete_task,
│                       // add_task_activity, list_task_activity, list_all_activity,
│                       // get_due_cron_tasks, update_task_cron_run,
│                       // set_task_agents, get_task_agents
├── projects.rs         // list_projects, create_project, update_project, delete_project,
│                       // set_project_agents, add_project_agent, get_project_agents,
│                       // update_project_agent_status, delete_agent, list_all_agents,
│                       // add_project_message, get_project_messages
├── trades.rs           // insert_trade, list_trades, daily_trade_summary
├── positions.rs        // insert_position, list_positions, update_position_price,
│                       // close_position, reduce_position, update_position_targets
└── embedding.rs        // bytes_to_f32_vec, f32_vec_to_bytes, cosine_similarity
```

### Pattern for Splitting `impl SessionStore`

Each module adds methods to `SessionStore` via a separate `impl` block with the struct imported:

```rust
// sessions/memories.rs
use super::SessionStore;

impl SessionStore {
    pub fn store_memory(&self, ...) -> ... { ... }
    pub fn search_memories_by_embedding(&self, ...) -> ... { ... }
    // etc.
}
```

This is standard Rust — multiple `impl` blocks in different files is idiomatic for large types.

### DRY Fixes During This Phase

| Duplication | Action |
|-------------|--------|
| Memory row-mapping repeated 6× (~60 lines) | Add `Memory::from_row(row: &Row) -> Result<Memory>` |
| Task row-mapping repeated 2× (~30 lines) | Add `Task::from_row(row: &Row) -> Result<Task>` + `load_task_agents()` |
| ProjectAgent row-mapping repeated 3× (~12 lines) | Add `ProjectAgent::from_row(row: &Row) -> Result<ProjectAgent>` |
| `list_positions` uses string interpolation for SQL | Change to parameterized query (security fix) |
| Schema DDL in `open()` is 284 lines inline | Move to `schema.rs` as const strings or a `run_migrations()` function |

### Call Sites

`SessionStore` is used everywhere — `commands.rs`, `agent_loop.rs`, `orchestrator.rs`, `memory.rs`, `channels.rs`, all channel bridges. The struct and its public API don't change — only import paths shift from `crate::engine::sessions::SessionStore` which still works via `mod.rs` re-export.

---

## Phase 4 — `sol_dex.rs` → `sol_dex/` (1,582 lines → 10 files)

Same pattern as `dex/`, but for the Solana chain.

### Target Structure

```
src-tauri/src/engine/sol_dex/
├── mod.rs              // re-exports
├── constants.rs        // KNOWN_TOKENS, JUPITER_API, PUMPPORTAL_API, TOKEN_PROGRAM_IDs,
│                       // slippage defaults
├── helpers.rs          // resolve_token, lamports_to_amount, amount_to_lamports
├── rpc.rs              // rpc_call, get_sol_balance, get_token_accounts,
│                       // get_mint_info, resolve_decimals_on_chain
├── wallet.rs           // pubkey_from_secret, execute_sol_wallet_create
├── portfolio.rs        // execute_sol_balance, execute_sol_portfolio
├── jupiter.rs          // execute_sol_quote, execute_sol_quote_jupiter,
│                       // execute_sol_swap, execute_sol_swap_jupiter
├── pumpportal.rs       // is_jupiter_route_error, pumpportal_get_tx, pumpportal_swap
├── transaction.rs      // sign_solana_transaction, decode_compact_u16,
│                       // encode_compact_u16, build_solana_transaction, derive_ata
├── transfer.rs         // execute_sol_transfer
└── price.rs            // get_token_price_usd (DexScreener)
```

### DRY Fixes During This Phase

| Duplication | Action |
|-------------|--------|
| Tx confirmation block copy-pasted 3× (L760, L1085, L1510) | Extract `fn check_tx_confirmation(rpc_url, tx_sig) -> String` |
| Keypair parsing duplicated 2× (L614, L1318) | Extract `fn parse_solana_keypair(encoded) -> Result<[u8; 64]>` |
| Slippage conversion `max(bps/100, 1)` 2× | Extract `fn slippage_pct(bps) -> u64` |

### Cross-Chain Sharing Opportunities (Future)

These are NOT required for this refactor but should be tracked:

| Shared Pattern | dex/ | sol_dex/ | Future shared module |
|---------------|------|----------|---------------------|
| JSON-RPC POST wrapper | `dex/rpc.rs` | `sol_dex/rpc.rs` | `crate::engine::rpc::json_rpc_call()` |
| Amount ↔ raw conversion | `dex/primitives.rs` | `sol_dex/helpers.rs` | Trait-based `TokenAmount` |
| DexScreener price lookup | — | `sol_dex/price.rs` | `crate::engine::price_oracle` |

---

## Phase 5 — `whatsapp.rs` → `whatsapp/` (1,223 lines → 7 files)

Half of this file is Docker/container infrastructure, not WhatsApp logic.

### Target Structure

```
src-tauri/src/engine/whatsapp/
├── mod.rs              // re-exports
├── config.rs           // WhatsAppConfig, Default impl, CONFIG_KEY,
│                       // load_config, save_config, approve_user, deny_user, remove_user
├── docker.rs           // EVOLUTION_IMAGE, CONTAINER_NAME, discover_colima_socket,
│                       // ensure_docker_ready, ensure_evolution_container
├── evolution_api.rs    // create_evolution_instance, extract_qr_from_response,
│                       // delete_evolution_instance, connect_evolution_instance,
│                       // send_whatsapp_message
├── webhook.rs          // run_webhook_listener (raw TCP HTTP server)
├── messages.rs         // handle_inbound_message
└── bridge.rs           // statics, get_stop_signal, start_bridge, stop_bridge,
                        // get_status, run_whatsapp_bridge
```

### DRY Fixes During This Phase

| Duplication | Action |
|-------------|--------|
| "Wait for API ready" polling loop appears 4× | Extract `async fn poll_api_ready(client, url, max_attempts) -> bool` |
| Docker force-remove pattern 3× | Extract `async fn force_remove_container(docker, name)` |
| `split_message()` duplicated from `channels.rs` | Delete local copy, import `channels::split_message` |

---

## Phase 6 — `telegram.rs` Cleanup (931 lines → `telegram/` or inline fixes)

Telegram is the smallest of the six, but it has the most **cross-file duplication** — 329 lines of `run_telegram_agent()` that duplicates `channels::run_channel_agent()`.

### Option A: Full split into `telegram/` (if more Telegram features are planned)

```
src-tauri/src/engine/telegram/
├── mod.rs              // re-exports
├── api_types.rs        // TgResponse, TgUpdate, TgMessage, TgUser, TgChat
├── api.rs              // TG_API, tg_get_me, tg_get_updates, tg_send_message, tg_send_chat_action
├── config.rs           // TelegramConfig, TelegramStatus, Default impl,
│                       // load_telegram_config, save_telegram_config,
│                       // approve_user, deny_user, remove_user
└── bridge.rs           // statics, start_bridge, stop_bridge, get_status, run_polling_loop
```

### Option B: Deduplicate only (if Telegram is stable)

1. Delete `run_telegram_agent()` (329 lines) → use `channels::run_channel_agent()` instead
2. Delete `split_message()` (22 lines) → use `channels::split_message()`
3. Delete `is_provider_billing_error()` (15 lines) → use `channels::is_provider_billing_error()`
4. Delete `PendingUser` struct → use `channels::PendingUser`
5. Migrate inline access-control in `run_polling_loop()` → use `channels::check_access`

This removes ~400 lines of duplication and brings the file to ~530 lines, which is reasonable for a single module.

### Recommendation

**Do Option B first** (dedup), then assess. 530 lines for a channel bridge is within budget. Only split into a directory if the file grows again.

---

## Phase Execution Order

| Phase | File | Lines | Risk | Dependencies |
|-------|------|-------|------|-------------|
| **1** | `dex.rs` → `dex/` | 2,822 | Low | Standalone — only called from `tools/dex.rs` |
| **2** | `skills.rs` → `skills/` | 1,700 | Medium | Called from 5+ modules. Must preserve `SessionStore` impls. |
| **3** | `sessions.rs` → `sessions/` | 1,949 | Medium-High | Central to everything. Careful `impl SessionStore` splitting. |
| **4** | `sol_dex.rs` → `sol_dex/` | 1,582 | Low | Standalone — only called from `tools/solana.rs` |
| **5** | `whatsapp.rs` → `whatsapp/` | 1,223 | Low | Self-contained bridge module |
| **6** | `telegram.rs` cleanup | 931 | Low | Just deduplication against `channels.rs` |

**Do phases 1 and 4 first** — they're standalone, low-risk, and the largest files. Phase 3 (sessions) is the riskiest because everything depends on `SessionStore`.

---

## Per-Phase Checklist

For each phase, the implementer must:

1. **Read this document's section** for the target structure
2. **Create the directory** and `mod.rs`
3. **Move code** function-by-function into new modules (don't rewrite logic)
4. **Apply DRY fixes** listed in the phase's table
5. **Update `engine/mod.rs`** — change `mod filename;` → `mod dirname;`
6. **Update all import sites** — `grep -rn "engine::oldname::" src/ --include="*.rs"`
7. **Run `cargo check`** — must compile clean with zero warnings
8. **Run `cargo test`** (if tests exist) — must pass
9. **Delete the old file** only after the above pass
10. **Verify git diff** — only import path changes outside the new directory

---

## ARCHITECTURE.md Updates Needed

After all phases complete, update `ARCHITECTURE.md`:

| Section | Change |
|---------|--------|
| Directory tree | Replace `tool_executor.rs` with `tools/` (17 modules). Replace `dex.rs`, `sol_dex.rs`, `skills.rs`, `sessions.rs`, `whatsapp.rs` with directory modules. |
| "Tool Executor" section | Rename to "Tools" — describe the `tools/` module system |
| Add "Extensibility Tiers" section | Document Skills / Integrations / Extensions as described above |
| Add "Community Skills" section | Document skills.sh, SKILL.md, search/install flow |
| LOC counts | Update after refactor |
| Database tables | Add `sessions`, `messages`, `config`, `memories`, `trade_history`, `positions`, `tasks`, `task_activity`, `community_skills` |

---

## Doc Site Fixes (Separate PR)

| File | Issue |
|------|-------|
| `docs/docs/guides/integrations.md` | Describes unbuilt TOML system as real. Add "Planned" callouts. Fix "AES-GCM" → "XOR cipher" for current implementation. |
| `docs/docs/guides/extensions.md` | Describes unbuilt Tier 3 as real. Add "Planned" callout. |
| `docs/docs/guides/skills.md` | Both this and `integrations.md` say "40 built-in" — clarify the distinction. |
| `docs/docs/channels/discord.md` | Line 47 says "via the Slack skill" — should say "via the Discord skill" |
| New: `docs/docs/reference/tools.md` | Missing — no page documents all 63 LLM-callable tools |

---

## Frontend Cleanup (Separate PR)

| File | Issue |
|------|-------|
| `src/views/skills.ts` (721 lines) | **100% stubbed out** — every action returns early with "coming soon" comments. Either delete entirely or merge any salvageable logic (risk scoring, safety dialogs) into `settings-skills.ts`. |
| `src/views/settings-skills.ts` | The working skills UI. Labels `Api` category as "API Integrations" — correct per docs but inconsistent with backend. No code changes needed now. |
| Sidebar nav icon | Uses `extension` Material Symbol for Skills — potentially confusing since "extension" means Tier 3 elsewhere. Low priority. |
