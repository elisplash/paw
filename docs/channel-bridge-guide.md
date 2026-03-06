# Channel Bridge Guide

This guide explains how channel bridges are structured in OpenPawz and what you need to touch when adding a new one.

The project already has a uniform pattern for most bridges. Reuse that pattern unless your target platform has a strong reason to be special, like Telegram's numeric user IDs and custom status type.

## What A Channel Bridge Does

A bridge connects an external chat platform to the local agent runtime. At a high level the flow is:

1. Receive a message from a channel
2. Check whether the sender is allowed
3. Route the message to the configured agent
4. Run the agent loop
5. Split the reply if the platform has message length limits
6. Send the response back to the channel

The shared helpers for that live in [`src-tauri/src/engine/channels/mod.rs`](../src-tauri/src/engine/channels/mod.rs).

## Standard Backend Contract

Most channels are expected to provide these functions:

- `start_bridge(app_handle)`
- `stop_bridge()`
- `get_status(&app_handle)`
- `load_config(&app_handle)`
- `save_config(&app_handle, &config)`
- `approve_user(&app_handle, &user_id)`
- `deny_user(&app_handle, &user_id)`
- `remove_user(&app_handle, &user_id)`

That contract is documented and consumed by the macro in [`src-tauri/src/commands/channels.rs`](../src-tauri/src/commands/channels.rs).

If your bridge matches this shape, you get the Tauri command layer almost for free.

## Shared Helpers You Should Reuse

From [`src-tauri/src/engine/channels/mod.rs`](../src-tauri/src/engine/channels/mod.rs):

- `load_channel_config()` and `save_channel_config()` for config persistence
- `run_channel_agent()` or `run_routed_channel_agent()` for message routing into the engine
- `split_message()` for platform message size limits
- `approve_user_generic()`, `deny_user_generic()`, and `remove_user_generic()` for access control flows

Do not duplicate these helpers in each bridge unless the platform genuinely needs a different behavior.

## File Touch Points For A New Standard Channel

### Rust backend

1. Create the engine module, usually `src-tauri/src/engine/<channel>.rs`
2. Export it from [`src-tauri/src/engine/mod.rs`](../src-tauri/src/engine/mod.rs)
3. Add a `channel_commands!(...)` entry in [`src-tauri/src/commands/channels.rs`](../src-tauri/src/commands/channels.rs)
4. Register the generated handler functions in [`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs)

### Frontend

1. Add config and status types in the shared frontend types if needed
2. Add typed IPC wrappers in [`src/engine/molecules/ipc_client.ts`](../src/engine/molecules/ipc_client.ts)
3. Add channel metadata to [`src/views/channels/atoms.ts`](../src/views/channels/atoms.ts)
4. Extend the switch statements in [`src/views/channels/molecules.ts`](../src/views/channels/molecules.ts)
5. Add setup UI handling in `src/views/channels/setup.ts` if the channel needs custom fields

## Suggested Backend Structure

A standard bridge file usually contains:

- Config struct
- Status struct
- Global runtime state for whether the bridge is running
- API client or socket helpers for the target platform
- A receive loop or callback that turns inbound messages into agent calls
- Access checks
- Response send helpers
- Public `start_bridge` and `stop_bridge` entry points

[`src-tauri/src/engine/telegram.rs`](../src-tauri/src/engine/telegram.rs) is useful as a detailed example, even though it is a special-case bridge.

## Access Control Pattern

Channel bridges are not just transport layers. They also enforce who may talk to the agent.

Common policy modes:

- `open`: anyone can message
- `allowlist`: only approved users may message
- `pairing`: first contact creates a pending request until a maintainer approves or denies it

If a platform can identify users reliably, it should support these access patterns unless there is a clear limitation.

## Frontend Channel Management Pattern

The frontend Channels screen treats bridges uniformly.

[`src/views/channels/atoms.ts`](../src/views/channels/atoms.ts):

- Declares human-facing setup metadata
- Defines form fields and config builders

[`src/views/channels/molecules.ts`](../src/views/channels/molecules.ts):

- Maps a channel name to `getConfig`, `setConfig`, `start`, `stop`, `status`, `approve`, and `deny` calls
- Renders cards and pending-user UI

[`src/views/channels/index.ts`](../src/views/channels/index.ts):

- Wires modal events
- Loads configured channels
- Handles auto-start behavior on boot

If you add a new bridge but forget one of these frontend switch points, the backend may work while the UI silently does not expose it.

## Telegram Is The Main Exception

Telegram is hand-written in [`src-tauri/src/commands/channels.rs`](../src-tauri/src/commands/channels.rs) because:

- User IDs are `i64`, not `String`
- Status type is custom
- Some user-management calls are async

That exception is useful as a reminder: follow the standard pattern first, and only break it when the platform API forces you to.

## New Bridge Checklist

1. Engine module created and exported
2. Config can load and save
3. Bridge can start, stop, and report status
4. Inbound messages run through shared access checks
5. Inbound messages route to an agent with `run_channel_agent()` or equivalent
6. Replies use `split_message()` where platform limits apply
7. Tauri commands are generated or added
8. `lib.rs` registers all command handlers
9. Frontend IPC client exposes the channel methods
10. Channels UI can configure, start, stop, and review pending users

## Recommended First Task

Before writing a whole new bridge, trace one existing bridge end to end:

1. Backend bridge module
2. `commands/channels.rs`
3. `lib.rs`
4. `ipc_client.ts`
5. `src/views/channels/atoms.ts`
6. `src/views/channels/molecules.ts`

Once that path is clear, adding a new bridge becomes mostly a consistency exercise rather than a discovery exercise.
