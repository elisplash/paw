---
sidebar_position: 16
title: "Dashboard & Today"
description: The Pawz dashboard with weather, tasks, quick actions, and agent greeting.
---

# Dashboard & Today

The Dashboard (also called the Today view) is your landing page in Pawz. It provides an at-a-glance overview of your day with a weather widget, task list, quick action buttons, and a personalized greeting from your active agent.

## Greeting

The dashboard displays a time-based greeting that changes throughout the day:

| Time | Greeting |
|------|----------|
| Before 12:00 PM | Good morning |
| 12:00 PM – 5:59 PM | Good afternoon |
| 6:00 PM onwards | Good evening |

The greeting is accompanied by your active agent's avatar.

## Weather widget

The dashboard includes a weather widget powered by [wttr.in](https://wttr.in).

- **No API key required** — wttr.in is a free, open service
- Displays current conditions for your location
- Updates automatically when the dashboard loads

:::tip
The wttr.in service uses your IP address for geolocation. If you're using a VPN, the weather will reflect the VPN server's location.
:::

## Task list

A lightweight task manager is built directly into the dashboard for quick daily planning.

### Operations

| Action | Description |
|--------|-------------|
| **Create** | Add a new task with a title |
| **Toggle** | Mark a task as complete/incomplete |
| **Edit** | Update a task's title |
| **Delete** | Remove a task from the list |

Tasks on the dashboard are stored in **localStorage** and are separate from the full Kanban task board (which uses the engine's SQLite database). Think of these as quick personal reminders rather than agent-managed tasks.

:::info
For full-featured task management with agent assignment, cron scheduling, and Kanban workflows, use the **Tasks** view instead.
:::

## Quick action buttons

The dashboard provides one-click quick actions that send pre-built prompts to your active agent:

| Button | What it does |
|--------|-------------|
| **Morning Briefing** | Asks the agent to prepare a morning summary with weather, calendar, tasks, and news |
| **Summarize Inbox** | Asks the agent to summarize recent emails and messages |
| **What's on today?** | Asks the agent to outline your agenda for the day |

These actions open the chat view and send the corresponding prompt immediately, so your agent can start working right away.

## Agent avatar display

Your currently active agent's avatar is displayed prominently on the dashboard. Pawz uses a sprite-based avatar system with 50 unique avatars, each assigned a color from a 7-color palette.

:::tip
Customize your agent's avatar and color in the **Agents** view to make the dashboard feel personal.
:::
