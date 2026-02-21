# Tools Refactor — Atomic File Structure

## Problem

Two god-files break the atomic architecture:

| File | Lines | What's in it |
|------|-------|-------------|
| `src-tauri/src/engine/tools.rs` | 1,606 | 53 tool **schema definitions** (all `ToolDefinition` constructors) + `builtins()` + `skill_tools()` |
| `src-tauri/src/engine/tool_executor.rs` | 2,707 | 53 tool **execution handlers** + giant `match` dispatcher + shared helpers |

**Total: 4,313 lines across 2 files for 53 tools.**

Every tool's schema lives in `tools.rs`, its executor lives in `tool_executor.rs`, and its name is hardcoded a third time in `src/features/agent-policies/atoms.ts` (`ALL_TOOLS`). Adding a tool requires editing all three. For open source, this guarantees merge conflicts.

---

## Target Structure

Convert `tools.rs` (single file) → `tools/` (directory module). Each tool group becomes **one file** containing both its schema definitions AND its execution handlers. The tool_executor.rs giant match becomes a thin dispatcher in `tools/mod.rs`.

```
src-tauri/src/engine/tools/
  mod.rs              ← registry, builtins(), skill_tools(), execute_tool() dispatcher
  exec.rs             ← exec (shell commands)
  fetch.rs            ← fetch (HTTP)
  filesystem.rs       ← read_file, write_file, list_directory, append_file, delete_file
  soul.rs             ← soul_read, soul_write, soul_list
  memory.rs           ← memory_store, memory_search
  web.rs              ← web_search, web_read, web_screenshot, web_browse
  email.rs            ← email_send, email_read
  telegram.rs         ← telegram_send, telegram_read
  slack.rs            ← slack_send, slack_read
  github.rs           ← github_api
  integrations.rs     ← rest_api_call, webhook_send, image_generate
  tasks.rs            ← create_task, list_tasks, manage_task
  agents.rs           ← create_agent, agent_list, agent_skills, agent_skill_assign, update_profile, self_info
  skills.rs           ← skill_search, skill_install, skill_list
  coinbase.rs         ← coinbase_prices, coinbase_balance, coinbase_wallet_create, coinbase_trade, coinbase_transfer
  dex.rs              ← dex_wallet_create, dex_balance, dex_quote, dex_swap, dex_portfolio, dex_token_info, dex_check_token, dex_search_token, dex_watch_wallet, dex_whale_transfers, dex_top_traders, dex_trending, dex_transfer
  solana.rs           ← sol_wallet_create, sol_balance, sol_quote, sol_swap, sol_portfolio, sol_token_info, sol_transfer
```

**17 files replacing 2.** Each file is ~80-250 lines max.

---

## Per-File Contract

Every tool module exports exactly two things:

```rust
/// Schema definitions for this tool group.
pub fn definitions() -> Vec<ToolDefinition> { ... }

/// Execute a tool call from this group. Returns None if the tool name doesn't belong here.
pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Option<Result<String, String>> { ... }
```

The `Option` return on `execute` lets the dispatcher try each module — `None` means "not my tool", `Some(result)` means "handled it".

---

## Detailed File Contents

### `tools/mod.rs` — Registry & Dispatcher

This file replaces both the old `tools.rs` `builtins()`/`skill_tools()` AND the old `tool_executor.rs` `execute_tool()` match.

```rust
// Paw Agent Engine — Tool Registry
// Each tool group is a self-contained module with definitions + executor.

use crate::atoms::types::*;
use log::info;

mod exec;
mod fetch;
mod filesystem;
mod soul;
mod memory;
mod web;
mod email;
mod telegram;
mod slack;
mod github;
mod integrations;
mod tasks;
mod agents;
mod skills;
mod coinbase;
mod dex;
mod solana;

/// All builtin tool modules (always available to agents).
const BUILTIN_MODULES: &[fn() -> Vec<ToolDefinition>] = &[
    exec::definitions,
    fetch::definitions,
    filesystem::definitions,
    soul::definitions,
    memory::definitions,
    web::definitions,
    tasks::definitions,
    agents::definitions,
    skills::definitions,
];

/// Return all builtin tool definitions.
pub fn builtins() -> Vec<ToolDefinition> {
    BUILTIN_MODULES.iter().flat_map(|f| f()).collect()
}

/// Return tool definitions for enabled skills.
pub fn skill_tools(enabled_skill_ids: &[String]) -> Vec<ToolDefinition> {
    let mut tools = Vec::new();
    for id in enabled_skill_ids {
        match id.as_str() {
            "email"      => tools.extend(email::definitions()),
            "slack"      => tools.extend(slack::definitions()),
            "telegram"   => tools.extend(telegram::definitions()),
            "github"     => tools.extend(github::definitions()),
            "rest_api" | "webhook" | "image_gen"
                         => tools.extend(integrations::definitions_for(id)),
            "coinbase"   => tools.extend(coinbase::definitions()),
            "dex"        => tools.extend(dex::definitions()),
            "solana_dex" => tools.extend(solana::definitions()),
            _ => {}
        }
    }
    tools
}

/// Execute a tool call — dispatches to the correct module.
pub async fn execute_tool(
    tool_call: &ToolCall,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> ToolResult {
    let name = &tool_call.function.name;
    let args_str = &tool_call.function.arguments;

    info!("[engine] Executing tool: {} agent={} args={}", name, agent_id, &args_str[..args_str.len().min(200)]);

    let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));

    // Try each module in order — first Some(result) wins.
    let result = None
        .or(exec::execute(name, &args, app_handle, agent_id).await)
        .or(fetch::execute(name, &args, app_handle, agent_id).await)
        .or(filesystem::execute(name, &args, app_handle, agent_id).await)
        .or(soul::execute(name, &args, app_handle, agent_id).await)
        .or(memory::execute(name, &args, app_handle, agent_id).await)
        .or(web::execute(name, &args, app_handle, agent_id).await)
        .or(tasks::execute(name, &args, app_handle, agent_id).await)
        .or(agents::execute(name, &args, app_handle, agent_id).await)
        .or(skills::execute(name, &args, app_handle, agent_id).await)
        .or(email::execute(name, &args, app_handle, agent_id).await)
        .or(telegram::execute(name, &args, app_handle, agent_id).await)
        .or(slack::execute(name, &args, app_handle, agent_id).await)
        .or(github::execute(name, &args, app_handle, agent_id).await)
        .or(integrations::execute(name, &args, app_handle, agent_id).await)
        .or(coinbase::execute(name, &args, app_handle, agent_id).await)
        .or(dex::execute(name, &args, app_handle, agent_id).await)
        .or(solana::execute(name, &args, app_handle, agent_id).await)
        .unwrap_or(Err(format!("Unknown tool: {}", name)));

    match result {
        Ok(output) => ToolResult {
            tool_call_id: tool_call.id.clone(),
            output,
            success: true,
        },
        Err(err) => ToolResult {
            tool_call_id: tool_call.id.clone(),
            output: format!("Error: {}", err),
            success: false,
        },
    }
}

/// Get the per-agent workspace directory path.
pub fn agent_workspace(agent_id: &str) -> std::path::PathBuf {
    let base = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join(".paw").join("workspaces").join(agent_id)
}

/// Ensure the agent's workspace directory exists.
pub fn ensure_workspace(agent_id: &str) -> Result<std::path::PathBuf, String> {
    let ws = agent_workspace(agent_id);
    std::fs::create_dir_all(&ws)
        .map_err(|e| format!("Failed to create workspace for agent '{}': {}", agent_id, e))?;
    Ok(ws)
}
```

### Example: `tools/filesystem.rs`

Shows the pattern every tool file follows. Schema + executor together.

```rust
// Paw Agent Engine — Filesystem Tools
// read_file, write_file, list_directory, append_file, delete_file

use crate::atoms::types::*;
use super::ensure_workspace;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "read_file".into(),
                description: "Read the contents of a file...".into(),
                parameters: serde_json::json!({ /* ... existing schema ... */ }),
            },
        },
        // ... write_file, list_directory, append_file, delete_file ...
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    _app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Option<Result<String, String>> {
    match name {
        "read_file" => Some(execute_read_file(args, agent_id).await),
        "write_file" => Some(execute_write_file(args, agent_id).await),
        "list_directory" => Some(execute_list_directory(args, agent_id).await),
        "append_file" => Some(execute_append_file(args, agent_id).await),
        "delete_file" => Some(execute_delete_file(args, agent_id).await),
        _ => None,  // not our tool
    }
}

// ── Private execution handlers (move from old tool_executor.rs as-is) ──

async fn execute_read_file(args: &serde_json::Value, agent_id: &str) -> Result<String, String> {
    // ... exact existing code from tool_executor.rs lines 297-338 ...
}

// ... etc for each tool ...
```

### Example: `tools/coinbase.rs`

Skill-based tool that needs credential checking:

```rust
// Paw Agent Engine — Coinbase CDP Tools

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use crate::engine::skills;
use tauri::Manager;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        // ... coinbase_prices, coinbase_balance, coinbase_wallet_create,
        //     coinbase_trade, coinbase_transfer schemas ...
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    _agent_id: &str,
) -> Option<Result<String, String>> {
    match name {
        "coinbase_prices" | "coinbase_balance" | "coinbase_wallet_create"
        | "coinbase_trade" | "coinbase_transfer" => {}
        _ => return None,  // not our tool
    }

    // All coinbase tools need credential check
    let creds = match check_skill_creds("coinbase", app_handle) {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };

    Some(match name {
        "coinbase_prices" => execute_coinbase_prices(args, &creds).await,
        "coinbase_balance" => execute_coinbase_balance(args, &creds).await,
        // ... etc ...
        _ => unreachable!(),
    })
}

/// Shared credential check for all coinbase tools.
fn check_skill_creds(
    skill_id: &str,
    app_handle: &tauri::AppHandle,
) -> Result<std::collections::HashMap<String, String>, String> {
    // ... move execute_skill_tool credential logic here ...
}

// ── Private execution handlers (moved from tool_executor.rs as-is) ──
// execute_coinbase_prices, execute_coinbase_balance, etc.
```

---

## Step-by-Step Execution Plan

### Step 1: Create the directory and mod.rs

1. `mkdir src-tauri/src/engine/tools` — but Rust won't have both `tools.rs` and `tools/`. So:
2. Delete `src-tauri/src/engine/tools.rs`
3. Create `src-tauri/src/engine/tools/mod.rs` with the registry code above.

### Step 2: Create each tool module

For each file below, **move** the schema from old `tools.rs` and the executor from old `tool_executor.rs`:

| New File | Schemas from `tools.rs` (lines) | Executors from `tool_executor.rs` (lines) |
|----------|------|------|
| `exec.rs` | `exec()` L11-30 | `execute_exec()` L132-221 |
| `fetch.rs` | `fetch()` L32-64 | `execute_fetch()` L222-296 |
| `filesystem.rs` | `read_file()` L66-86, `write_file()` L87-111, `list_directory()` L112-139, `append_file()` L140-164, `delete_file()` L165-188 | `execute_read_file()` L297-338, `execute_write_file()` L339-382, `execute_list_directory()` L383-463, `execute_append_file()` L464-495, `execute_delete_file()` L496-534 |
| `soul.rs` | `soul_read()` L190-210, `soul_write()` L211-235, `soul_list()` L236-247 | `execute_soul_read()` L535-550, `execute_soul_write()` L551-577, `execute_soul_list()` L578-598 |
| `memory.rs` | `memory_store()` L248-273, `memory_search()` L274-299 | `execute_memory_store()` L599-616, `execute_memory_search()` L617-642 |
| `web.rs` | `web_search()` L490-507, `web_read()` L508-525, `web_screenshot()` L526-545, `web_browse()` L546-571 | These already live in `engine/web.rs` — just call through. Executor is: `web::execute_web_search`, `web::execute_web_read`, `web::execute_web_screenshot`, `web::execute_web_browse` |
| `email.rs` | `email_send()` L300-319, `email_read()` L320-336 | `execute_email_send()` L1594-1665, `execute_email_read()` L1666-1710 |
| `telegram.rs` | `telegram_send()` L337-355, `telegram_read()` L356-371 | `execute_telegram_send()` L2585-2667, `execute_telegram_read()` L2668-2707 |
| `slack.rs` | `slack_send()` L372-389, `slack_read()` L390-407 | `execute_slack_send()` L1711-1753, `execute_slack_read()` L1754-1797 |
| `github.rs` | `github_api()` L408-426 | `execute_github_api()` L1798-1857 |
| `integrations.rs` | `rest_api_call()` L427-446, `webhook_send()` L447-463, `image_generate()` L464-489 | `execute_rest_api_call()` L1858-1920, `execute_webhook_send()` L1921-1973, `execute_image_generate()` L1974-~2040 |
| `tasks.rs` | `create_task()` L615-652, `list_tasks()` L653-676, `manage_task()` L677-708 | `execute_create_task()` L842-911, `execute_list_tasks()` L912-952, `execute_manage_task()` L953-1047 |
| `agents.rs` | `create_agent()` L572-614, `update_profile()` L709-745, `self_info()` L746-757, `agent_list()` L1489-1499, `agent_skills()` L1500-1519, `agent_skill_assign()` L1520-1550 | `execute_create_agent()` L771-841, `execute_update_profile()` L717-770, `execute_self_info()` L643-716, `execute_agent_list()` L1048-1104, `execute_agent_skills()` L1105-1149, `execute_agent_skill_assign()` L1150-1231 |
| `skills.rs` | `skill_search()` L1551-1570, `skill_install()` L1571-1594, `skill_list()` L1595-1606 | `execute_skill_search()` L1232-1277, `execute_skill_install()` L1278-1303, `execute_skill_list()` L1304-1340 |
| `coinbase.rs` | `coinbase_prices()` L842-861, `coinbase_balance()` L862-880, `coinbase_wallet_create()` L881-900, `coinbase_trade()` L901-942, `coinbase_transfer()` L943-980 | `execute_coinbase_prices()` L2336-2364, `execute_coinbase_balance()` L2365-2417, `execute_coinbase_wallet_create()` L2418-2443, `execute_coinbase_trade()` L2444-2520, `execute_coinbase_transfer()` L2521-2584, `cdp_request()` L2287-2335 (shared helper — lives in this file) |
| `dex.rs` | `dex_wallet_create()` L981-991, `dex_balance()` L992-1010, `dex_quote()` L1011-1046, `dex_swap()` L1047-1086, `dex_portfolio()` L1087-1106, `dex_token_info()` L1107-1126, `dex_check_token()` L1127-1146, `dex_search_token()` L1147-1174, `dex_watch_wallet()` L1175-1203, `dex_whale_transfers()` L1204-1231, `dex_top_traders()` L1232-1259, `dex_trending()` L1260-1282, `dex_transfer()` L1283-1316 | Executors are already in `engine/dex.rs` — called as `crate::engine::dex::execute_dex_*`. This file only needs the schemas + thin dispatch that calls through + trade recording logic from `execute_skill_tool` match arms |
| `solana.rs` | `sol_wallet_create()` L1317-1327, `sol_balance()` L1328-1346, `sol_quote()` L1347-1378, `sol_swap()` L1379-1414, `sol_portfolio()` L1415-1434, `sol_token_info()` L1435-1454, `sol_transfer()` L1455-1488 | Executors already in `engine/sol_dex.rs` — called as `crate::engine::sol_dex::execute_sol_*`. Same pattern as dex — schemas + dispatch + trade recording + position auto-open logic |

### Step 3: Move shared helpers

- `agent_workspace()` and `ensure_workspace()` from `tool_executor.rs` L20-31 → `tools/mod.rs` (public, used by filesystem/soul/exec modules)
- `execute_skill_tool()` credential check logic from `tool_executor.rs` L1341-1380 → extract as a shared function `check_skill_credentials()` in `tools/mod.rs`, used by `coinbase.rs`, `dex.rs`, `solana.rs`, `email.rs`, `slack.rs`, `github.rs`, `integrations.rs`

### Step 4: Delete old files

- Delete `src-tauri/src/engine/tools.rs` (replaced by `tools/mod.rs` + modules)
- Delete `src-tauri/src/engine/tool_executor.rs` (absorbed into `tools/`)

### Step 5: Update module declarations

**`src-tauri/src/engine/mod.rs`:**
- Remove `pub mod tool_executor;` (no longer exists)
- `pub mod tools;` stays (now points to `tools/` directory)

### Step 6: Update all external references

There are exactly 5 call sites:

| File | Old | New |
|------|-----|-----|
| `engine/agent_loop.rs` L7 | `use crate::engine::tool_executor;` | `use crate::engine::tools;` |
| `engine/agent_loop.rs` L379 | `tool_executor::execute_tool(tc, ...)` | `tools::execute_tool(tc, ...)` |
| `engine/chat.rs` L16 | `use crate::engine::tool_executor;` | `use crate::engine::tools;` |
| `engine/chat.rs` L113 | `tool_executor::agent_workspace(agent_id)` | `tools::agent_workspace(agent_id)` |
| `engine/orchestrator.rs` L933, L1313 | `crate::engine::tool_executor::execute_tool(tc, ...)` | `crate::engine::tools::execute_tool(tc, ...)` |

### Step 7: `cargo check`

Build must pass. The refactor is 100% behavioral no-op — same schemas, same execution, same function signatures, just reorganized.

---

## Rules for Each Tool File

1. **Schema + executor live together.** One file = one tool group. No exceptions.
2. **No cross-tool-file imports.** Tool files import from `super::` (mod.rs), `crate::atoms::types`, and their engine backends (`crate::engine::dex`, etc). Never from each other.
3. **definitions() returns Vec<ToolDefinition>.** Exact same schemas as today. Don't change any schema content.
4. **execute() returns Option<Result<String, String>>.** Return `None` for unknown tool names. Return `Some(Ok(...))` or `Some(Err(...))` for handled tools.
5. **Move existing code verbatim.** This is a pure refactor — no logic changes, no renaming functions, no changing behavior. Copy-paste the executor code as-is.
6. **Skill tools need credential checking.** For email, slack, github, integrations, coinbase, dex, solana — use the shared `check_skill_credentials()` helper from mod.rs.

---

## What NOT to Change

- `src-tauri/src/atoms/types.rs` — `ToolDefinition`, `ToolResult`, etc. structs stay exactly where they are
- `src/features/agent-policies/atoms.ts` — `ALL_TOOLS` list stays (it's frontend, will be addressed in Phase 2)
- `engine/chat.rs` — `build_chat_tools()` logic stays the same, just update import path
- `engine/web.rs`, `engine/dex.rs`, `engine/sol_dex.rs` — these engine backends don't move. The new tool files call into them.
- Tool schemas — zero content changes to any schema. Same descriptions, same parameters, same JSON.
