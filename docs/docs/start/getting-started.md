---
sidebar_position: 1
title: What is Pawz?
slug: getting-started
---

# What is Pawz?

Pawz is a **native desktop AI agent platform** built on [Tauri v2](https://v2.tauri.app/). It gives you a standalone app where you can create AI agents, connect them to chat platforms, and let them work autonomously — all without opening a terminal or editing config files.

## How it works

```
┌──────────────────────┐          ┌──────────────────────────────┐
│  Pawz Desktop UI     │          │  Rust Backend Engine         │
│  (TypeScript)        │◄── IPC ──►  Agent loop + tool executor  │
│                      │          │  10 channel bridges          │
│  20+ views           │          │  10 AI providers             │
│  Material Symbols    │          │  SQLite + OS keychain        │
└──────────────────────┘          └──────────────────────────────┘
```

Everything runs locally on your machine through Tauri IPC — no open ports, no Node.js backend, no cloud dependency.

## What you can do

- **Chat with AI agents** — each with their own personality, model, and tools
- **Connect to 10 chat platforms** — Telegram, Discord, Slack, Matrix, IRC, and more
- **Use 10+ AI providers** — Ollama (local), OpenAI, Anthropic, Google, DeepSeek, and more
- **Build with 37+ skills** — email, GitHub, trading, TTS, image generation, smart home
- **Orchestrate multi-agent projects** — boss/worker pattern with task delegation
- **Manage tasks** — Kanban board with agent assignment and cron scheduling
- **Research** — dedicated workflow with findings and synthesis reports
- **Remember everything** — semantic long-term memory with auto-recall
- **Stay secure** — command risk classification, approval modals, container sandboxing

## Quick links

- [Installation](./installation) — prerequisites and build instructions
- [Create your first agent](./first-agent) — up and running in 2 minutes
- [Connect a provider](./first-provider) — add an AI model
- [Guides](../guides/agents) — deep dives into every feature
- [Channels](../channels/overview) — connect to Telegram, Discord, and more
