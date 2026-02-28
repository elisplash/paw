// ─────────────────────────────────────────────────────────────────────────────
// Flow Templates — Core (AI & Agents, Communication, DevOps)
// Pure data, no DOM, no IPC.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowTemplate } from './atoms';

export const TEMPLATES_CORE: FlowTemplate[] = [
  // ── AI & Agents ──────────────────────────────────────────────────────────

  {
    id: 'tpl-daily-digest',
    name: 'Daily Digest',
    description: 'Summarize activity from multiple channels into a single daily briefing.',
    category: 'ai',
    tags: ['summary', 'daily', 'channels', 'briefing'],
    icon: 'summarize',
    nodes: [
      {
        kind: 'trigger',
        label: 'Daily Schedule',
        description: 'Cron: 8:00 AM',
        config: { prompt: 'Run every morning at 8 AM' },
      },
      {
        kind: 'agent',
        label: 'Gather Activity',
        description: 'Fetch from all channels',
        config: {
          prompt:
            'Collect messages, events, and updates from all connected channels since yesterday',
        },
      },
      {
        kind: 'agent',
        label: 'Summarize',
        description: 'Create digest',
        config: {
          prompt:
            'Create a concise daily digest with key highlights, action items, and mentions. Group by channel.',
        },
      },
      {
        kind: 'output',
        label: 'Post Digest',
        description: 'Send to Slack/Discord',
        config: { outputTarget: 'chat' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
      { fromIdx: 2, toIdx: 3 },
    ],
  },

  {
    id: 'tpl-multi-agent-debate',
    name: 'Multi-Agent Debate',
    description: 'Two agents debate a topic, a third summarizes the outcome.',
    category: 'ai',
    tags: ['multi-agent', 'debate', 'analysis', 'reasoning'],
    icon: 'group',
    nodes: [
      {
        kind: 'trigger',
        label: 'Topic Input',
        description: 'User provides topic',
        config: { prompt: 'User enters a topic or question to debate' },
      },
      {
        kind: 'agent',
        label: 'Agent Pro',
        description: 'Argues for',
        config: {
          prompt:
            'Present the strongest arguments IN FAVOR of the topic. Be thorough and evidence-based.',
        },
      },
      {
        kind: 'agent',
        label: 'Agent Con',
        description: 'Argues against',
        config: {
          prompt:
            'Present the strongest arguments AGAINST the topic. Be thorough and evidence-based.',
        },
      },
      {
        kind: 'agent',
        label: 'Moderator',
        description: 'Synthesize',
        config: {
          prompt:
            'Analyze both sides of the debate. Identify the strongest points, weaknesses, and produce a balanced conclusion.',
        },
      },
      {
        kind: 'output',
        label: 'Verdict',
        description: 'Final analysis',
        config: { outputTarget: 'chat' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 0, toIdx: 2 },
      { fromIdx: 1, toIdx: 3 },
      { fromIdx: 2, toIdx: 3 },
      { fromIdx: 3, toIdx: 4 },
    ],
  },

  {
    id: 'tpl-agent-chain',
    name: 'Agent Chain',
    description: 'Sequential multi-agent pipeline — each agent refines the previous output.',
    category: 'ai',
    tags: ['chain', 'pipeline', 'refinement', 'multi-agent'],
    icon: 'linked_services',
    nodes: [
      { kind: 'trigger', label: 'Input', description: 'User prompt', config: {} },
      {
        kind: 'agent',
        label: 'Draft',
        description: 'First pass',
        config: { prompt: 'Generate an initial draft response to the input.' },
      },
      {
        kind: 'agent',
        label: 'Review',
        description: 'Critique & improve',
        config: {
          prompt: 'Review the draft critically. Fix errors, improve clarity, add missing details.',
        },
      },
      {
        kind: 'agent',
        label: 'Polish',
        description: 'Final polish',
        config: {
          prompt:
            'Final polish: ensure tone is professional, formatting is clean, and the response is complete.',
        },
      },
      {
        kind: 'output',
        label: 'Result',
        description: 'Polished output',
        config: { outputTarget: 'chat' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
      { fromIdx: 2, toIdx: 3 },
      { fromIdx: 3, toIdx: 4 },
    ],
  },

  // ── Communication ────────────────────────────────────────────────────────

  {
    id: 'tpl-email-responder',
    name: 'Email Auto-Responder',
    description: 'Agent drafts email replies, routes based on urgency.',
    category: 'communication',
    tags: ['email', 'auto-reply', 'triage', 'inbox'],
    icon: 'mail',
    nodes: [
      {
        kind: 'trigger',
        label: 'New Email',
        description: 'IMAP trigger',
        config: { prompt: 'Triggered when a new email arrives' },
      },
      {
        kind: 'agent',
        label: 'Analyze Email',
        description: 'Classify urgency',
        config: {
          prompt:
            'Analyze the email: classify urgency (high/medium/low), extract key topics, determine if it needs a reply.',
        },
      },
      {
        kind: 'condition',
        label: 'Needs Reply?',
        config: { conditionExpr: 'Does the email require a response?' },
      },
      {
        kind: 'agent',
        label: 'Draft Reply',
        description: 'Compose response',
        config: { prompt: 'Draft a professional reply to this email. Match the tone and context.' },
      },
      {
        kind: 'output',
        label: 'Send Reply',
        description: 'SMTP send',
        config: { outputTarget: 'chat' },
      },
      {
        kind: 'output',
        label: 'Archive',
        description: 'No reply needed',
        config: { outputTarget: 'log' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
      { fromIdx: 2, toIdx: 3, label: 'Yes' },
      { fromIdx: 2, toIdx: 5, label: 'No' },
      { fromIdx: 3, toIdx: 4 },
    ],
  },

  {
    id: 'tpl-channel-bridge',
    name: 'Channel Bridge',
    description: 'Forward and translate messages between two platforms.',
    category: 'communication',
    tags: ['bridge', 'cross-platform', 'sync', 'channels'],
    icon: 'swap_horiz',
    nodes: [
      {
        kind: 'trigger',
        label: 'Source Channel',
        description: 'Slack / Discord / etc.',
        config: { prompt: 'Watch for new messages in the source channel' },
      },
      {
        kind: 'agent',
        label: 'Format Message',
        description: 'Adapt format',
        config: {
          prompt:
            'Reformat the message for the destination platform. Preserve meaning, adjust formatting and mentions.',
        },
      },
      {
        kind: 'output',
        label: 'Destination',
        description: 'Post to target',
        config: { outputTarget: 'chat' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
    ],
  },

  {
    id: 'tpl-meeting-notes',
    name: 'Meeting Notes',
    description: 'Transcribe and summarize meeting recordings.',
    category: 'communication',
    tags: ['meeting', 'transcription', 'notes', 'voice'],
    icon: 'record_voice_over',
    nodes: [
      {
        kind: 'trigger',
        label: 'Recording',
        description: 'Audio/video input',
        config: { prompt: 'Triggered when a meeting recording is provided' },
      },
      {
        kind: 'agent',
        label: 'Transcribe',
        description: 'Speech to text',
        config: {
          prompt:
            'Transcribe the meeting recording accurately. Include speaker labels where possible.',
        },
      },
      {
        kind: 'agent',
        label: 'Summarize',
        description: 'Extract key points',
        config: {
          prompt:
            'Summarize the meeting: key decisions, action items with owners, follow-ups, and deadlines.',
        },
      },
      {
        kind: 'output',
        label: 'Notes',
        description: 'Post summary',
        config: { outputTarget: 'chat' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
      { fromIdx: 2, toIdx: 3 },
    ],
  },

  // ── DevOps ───────────────────────────────────────────────────────────────

  {
    id: 'tpl-pr-reviewer',
    name: 'PR Code Reviewer',
    description: 'Agent reviews pull requests, posts comments with suggestions.',
    category: 'devops',
    tags: ['github', 'code-review', 'pull-request', 'ci'],
    icon: 'code',
    nodes: [
      {
        kind: 'trigger',
        label: 'PR Opened',
        description: 'GitHub webhook',
        config: { prompt: 'Triggered when a pull request is opened or updated' },
      },
      {
        kind: 'tool',
        label: 'Fetch Diff',
        description: 'Get PR changes',
        config: { prompt: 'Fetch the pull request diff and changed files' },
      },
      {
        kind: 'agent',
        label: 'Review Code',
        description: 'Analyze changes',
        config: {
          prompt:
            'Review the code changes. Check for bugs, security issues, performance problems, and style. Be constructive.',
        },
      },
      {
        kind: 'condition',
        label: 'Issues Found?',
        config: { conditionExpr: 'Were any issues or suggestions identified?' },
      },
      {
        kind: 'tool',
        label: 'Post Review',
        description: 'Post comments',
        config: { prompt: 'Post inline review comments on the PR with specific suggestions' },
      },
      {
        kind: 'output',
        label: 'Approve',
        description: 'PR looks good',
        config: { outputTarget: 'log' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
      { fromIdx: 2, toIdx: 3 },
      { fromIdx: 3, toIdx: 4, label: 'Yes' },
      { fromIdx: 3, toIdx: 5, label: 'No' },
    ],
  },

  {
    id: 'tpl-incident-response',
    name: 'Incident Response',
    description: 'Automated incident triage, notification, and status tracking.',
    category: 'devops',
    tags: ['incident', 'alert', 'monitoring', 'ops'],
    icon: 'crisis_alert',
    nodes: [
      {
        kind: 'trigger',
        label: 'Alert Fired',
        description: 'Monitoring webhook',
        config: { prompt: 'Triggered by monitoring alert (PagerDuty, Grafana, etc.)' },
      },
      {
        kind: 'agent',
        label: 'Triage',
        description: 'Classify severity',
        config: {
          prompt:
            'Analyze the alert: classify severity (P1-P4), identify affected systems, suggest initial response.',
        },
      },
      {
        kind: 'condition',
        label: 'Severity P1/P2?',
        config: { conditionExpr: 'Is the severity P1 or P2?' },
      },
      {
        kind: 'output',
        label: 'Page On-Call',
        description: 'Urgent notification',
        config: { outputTarget: 'chat' },
      },
      {
        kind: 'output',
        label: 'Log & Monitor',
        description: 'Non-urgent',
        config: { outputTarget: 'log' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
      { fromIdx: 2, toIdx: 3, label: 'Yes' },
      { fromIdx: 2, toIdx: 4, label: 'No' },
    ],
  },

  {
    id: 'tpl-deploy-pipeline',
    name: 'Deploy Pipeline',
    description: 'Agent-driven deployment with checks and rollback.',
    category: 'devops',
    tags: ['deploy', 'ci-cd', 'rollback', 'release'],
    icon: 'rocket_launch',
    nodes: [
      { kind: 'trigger', label: 'Deploy Request', description: 'Manual or CI trigger', config: {} },
      {
        kind: 'tool',
        label: 'Run Tests',
        description: 'Execute test suite',
        config: { prompt: 'Run the full test suite and report results' },
      },
      { kind: 'condition', label: 'Tests Pass?', config: { conditionExpr: 'Did all tests pass?' } },
      {
        kind: 'tool',
        label: 'Deploy',
        description: 'Push to production',
        config: { prompt: 'Deploy the application to the target environment' },
      },
      {
        kind: 'agent',
        label: 'Verify',
        description: 'Health check',
        config: {
          prompt:
            'Verify the deployment: run smoke tests, check health endpoints, confirm metrics are normal.',
        },
      },
      {
        kind: 'output',
        label: 'Success',
        description: 'Notify team',
        config: { outputTarget: 'chat' },
      },
      {
        kind: 'output',
        label: 'Failed',
        description: 'Tests failed',
        config: { outputTarget: 'chat' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
      { fromIdx: 2, toIdx: 3, label: 'Pass' },
      { fromIdx: 2, toIdx: 6, label: 'Fail' },
      { fromIdx: 3, toIdx: 4 },
      { fromIdx: 4, toIdx: 5 },
    ],
  },
];
