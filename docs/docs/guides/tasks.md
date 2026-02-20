---
sidebar_position: 3
title: Tasks
---

# Tasks

The Tasks view is a kanban board for managing work items that agents can pick up and execute.

## Board columns

| Column | Status | Meaning |
|--------|--------|---------|
| **Inbox** | `inbox` | New tasks, unassigned |
| **Assigned** | `assigned` | Agent assigned, not started |
| **In Progress** | `in_progress` | Agent actively working |
| **Review** | `review` | Completed, needs human review |
| **Blocked** | `blocked` | Waiting on something |
| **Done** | `done` | Finished |

Drag and drop tasks between columns to change status.

## Creating a task

1. Click **New Task**
2. Fill in:
   - **Title** — what needs to be done
   - **Description** — details and context
   - **Priority** — low / medium / high / urgent
   - **Assigned agent** — which agent handles it
   - **Model override** — use a specific model for this task

## Priority levels

| Priority | When to use |
|----------|-------------|
| `low` | Nice to have, no deadline |
| `medium` | Standard priority |
| `high` | Important, do soon |
| `urgent` | Do immediately |

## Multi-agent assignment

Tasks can have multiple agents:

- **Lead** — primary agent responsible for the task
- **Collaborator** — assists the lead agent

## Scheduling with cron

Tasks can run on a schedule:

1. Open a task
2. Set a **cron schedule** (standard cron syntax)
3. Enable **cron**

The engine checks for due tasks every 60 seconds.

### Cron examples

```
0 9 * * *       Every day at 9 AM
0 */2 * * *     Every 2 hours
0 9 * * 1-5     Weekdays at 9 AM
*/30 * * * *    Every 30 minutes
```

## Activity feed

Each task has a live activity feed showing:

| Event | Description |
|-------|-------------|
| `created` | Task was created |
| `assigned` | Agent was assigned |
| `status_change` | Task moved to a new column |
| `comment` | Human added a note |
| `agent_started` | Agent began working |
| `agent_completed` | Agent finished |
| `agent_error` | Agent encountered an error |
| `cron_triggered` | Scheduled run fired |

Filter the feed by: **All**, **Tasks only**, or **Status changes**.

## Automation

See the [Automations guide](./automations) for setting up recurring agent tasks.
