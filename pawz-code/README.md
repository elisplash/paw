# Pawz CODE

A standalone developer AI agent — isolated from the main Pawz app so it can work on the OpenPawz codebase without ever breaking itself.

## The idea

Pawz Desktop is the full platform. **Pawz CODE** is a stripped sidecar that:

- Runs as a background service on your machine
- Knows your codebase inside out (persistent Engram memory in `~/.pawz-code/`)
- Has full code tools: `exec`, `read_file`, `write_file`, `list_directory`, `grep`, `fetch`
- Exposes the exact same `/chat/stream` SSE endpoint the VS Code extension already speaks
- Has **zero dependency** on the Tauri app, SQLite schema, or channels it's working on

If the OpenPawz build explodes, Pawz CODE keeps running. The surgeon never stands in the operating room.

---

## Structure

```
pawz-code/
  server/                  Rust binary — the agent service
    Cargo.toml
    src/
      main.rs              HTTP server (axum), auth middleware, SSE route
      config.rs            ~/.pawz-code/config.toml
      state.rs             AppState: config + SQLite + broadcast channel
      types.rs             EngineEvent (same wire format as Pawz Desktop)
      memory.rs            Conversation history + pinned notes (SQLite)
      tools.rs             exec, read_file, write_file, list_directory, grep, fetch, remember, recall
      provider.rs          Anthropic + OpenAI-compatible streaming parsers
      agent.rs             The agent loop: LLM → tools → LLM → ...

  vscode-extension/        VS Code extension — use @code in chat
    package.json           Contributes chatParticipant id "pawz-code" (@code)
    tsconfig.json
    src/
      extension.ts         Chat participant, diff command, workspace context injection
      pawz-client.ts       SSE streaming client (same protocol as Pawz Desktop)
      tool-renderer.ts     Maps EngineEvents → VS Code ChatResponseStream
```

---

## Quick Start

### 1. Build the server (one-time)

```bash
cd server
cargo build --release
```

### 2. Start the Control Panel

```bash
cd app
pnpm install  # first time only
pnpm run tauri dev
```

The control panel will:
- ✅ Auto-start the daemon
- ✅ Display your auth token with a copy button
- ✅ Show daemon status and logs

### 3. Copy the auth token

In the control panel UI, click **"📋 Copy Token"** at the top.

### 4. Install VS Code extension

```bash
cd vscode-extension
code --install-extension pawz-code-0.1.0.vsix
```

Or in VS Code: **Extensions → ⋯ → Install from VSIX** → select `pawz-code-0.1.0.vsix`.

### 5. Paste token in VS Code

Open VS Code settings (`Cmd+,`) and search for `pawzCode.authToken`, then paste the token.

### 6. Test it!

In VS Code chat: `@pawzcode hello!`

Then try a follow-up: `what files are in this workspace?`

The agent should respond to both messages (session persistence is now working).

---

## Configuration

The control panel provides a configuration editor, or you can manually edit `~/.pawz-code/config.toml`:

```toml
port = 3941
bind = "127.0.0.1"
auth_token = "auto-generated"
provider = "anthropic"
api_key = ""           # ← add your API key here
model = "claude-sonnet-4-20250514"
max_rounds = 20
workspace_root = ""    # ← optional: absolute path to your repo
```

### Supported Providers

| `provider`     | `base_url`            | Models                          |
|----------------|----------------------|----------------------------------|
| `anthropic`    | (auto)               | `claude-sonnet-4-20250514`, etc.|
| `openai`       | (auto)               | `gpt-4o`, `o1`, etc.            |
| `openai`       | `http://localhost:11434` | any Ollama model            |
| `openai`       | `https://openrouter.ai/api` | any OpenRouter model       |
| `claude_code`  | (none)               | uses your Claude Code install  |

### Using `claude_code` provider

`claude_code` routes requests through the Claude Code CLI instead of requiring an API key.

Requirements:
1. Install Claude Code: `npm install -g @anthropic-ai/claude-code`
2. Authenticate: `claude login` (one-time, opens browser)
3. Set in control panel or `~/.pawz-code/config.toml`:

```toml
provider = "claude_code"
# api_key is not needed
# Optionally override binary path:
# claude_binary_path = "/usr/local/bin/claude"
```

---

## Manual Daemon Control

If you prefer not to use the control panel:

```bash
cd server
cargo run
# or
./target/release/pawz-code
```

The daemon will print the auth token on first run. Copy it to VS Code settings.

---

## Tools available

| Tool | What it does |
|------|-------------|
| `exec` | Run any shell command (`git`, `cargo`, `pnpm`, `gh`, etc.) |
| `read_file` | Read file contents, optionally by line range |
| `write_file` | Write/overwrite a file, creates parent dirs |
| `list_directory` | List directory contents, optionally recursive |
| `grep` | Regex search across files with context lines |
| `fetch` | HTTP GET/POST for docs, APIs, URLs |
| `remember` | Persist a named note to long-term memory |
| `recall` | Search long-term memory notes |

---

## Memory

All conversation history is stored in `~/.pawz-code/memory.db` (SQLite). The agent builds a persistent picture of the codebase across sessions via `remember` calls.

When the container restarts, the server boots and loads the same DB — no context lost.

---

## Why separate from Pawz Desktop?

Pawz Desktop manages Discord, Telegram, trading, OAuth, n8n flows, and dozens of other things. Pawz CODE only needs a model, a DB, and code tools. Keeping them separate means:

- No shared port conflicts
- No shared DB schema coupling
- If OpenPawz refactors its engine, Pawz CODE is unaffected
- Pawz CODE can be deployed in any Docker container or CI environment
- Operator access to the Tauri app is not required
