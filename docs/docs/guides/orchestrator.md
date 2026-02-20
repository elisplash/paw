---
sidebar_position: 7
title: Orchestrator
---

# Orchestrator

The Orchestrator enables multi-agent projects where a boss agent delegates tasks to worker agents.

## Concepts

| Role | Model | Description |
|------|-------|-------------|
| **Boss** | `boss_model` (expensive, powerful) | Plans, delegates, reviews results |
| **Worker** | `worker_model` (cheap, fast) | Executes specific tasks |

## Creating a project

1. Go to **Orchestrator** in the sidebar
2. Click **New Project**
3. Fill in:
   - **Title** — project name
   - **Goal** — what the project should accomplish
   - **Boss agent** — the agent that leads
   - **Workers** — add agents with specialties

## Agent specialties

| Specialty | Best for |
|-----------|----------|
| `coder` | Writing and reviewing code |
| `researcher` | Finding information, analysis |
| `designer` | Creative work, content |
| `communicator` | Writing emails, messages, docs |
| `security` | Security reviews, audits |
| `general` | Anything |

Specialty models can be configured in **Settings → Engine** under Model Routing. For example, setting `coder` to `gemini-2.5-pro` routes all coding tasks to that model.

## How it works

```
Boss agent gets goal + team roster
        ↓
Boss plans and calls delegate_task(agent, task)
        ↓
Worker spawns async (tokio::spawn)
        ↓
Worker executes with worker_model
        ↓
Worker calls report_progress(status, result)
        ↓
Boss reviews and delegates more or calls project_complete
```

## Boss tools

| Tool | Description |
|------|-------------|
| `delegate_task` | Assign work to a worker agent |
| `check_agent_status` | See what a worker is doing |
| `send_agent_message` | Send a message to a worker |
| `project_complete` | Mark the project as done |
| `create_sub_agent` | Create a new agent on the fly |

## Worker tools

Workers have access to all standard safe tools plus:
- `report_progress` — send status updates back to the boss
  - Statuses: `working`, `done`, `error`, `blocked`

Workers can also use: email, slack, GitHub, REST API, webhooks, and image generation without HIL approval.

## Project statuses

| Status | Meaning |
|--------|---------|
| `planning` | Created, not started |
| `running` | Boss is actively delegating |
| `paused` | Manually paused |
| `completed` | Boss called project_complete |
| `failed` | Unrecoverable error |

## Message bus

The project detail view shows a real-time message bus (polled every 3s):

| Message type | From |
|-------------|------|
| `delegation` | Boss assigned a task |
| `progress` | Worker reporting status |
| `result` | Worker completed task |
| `error` | Something went wrong |
| `message` | Inter-agent communication |

## Model routing

Configure which models handle what in **Settings → Engine**:

| Setting | Purpose |
|---------|---------|
| `boss_model` | Model for the boss agent |
| `worker_model` | Default model for workers |
| `specialty_models` | Per-specialty overrides (e.g., coder → gemini-2.5-pro) |
| `agent_models` | Per-agent overrides (highest priority) |
| `auto_tier` | Automatically select cheap models for simple tasks |

**Resolution priority:** agent_models > specialty_models > role-based > fallback
