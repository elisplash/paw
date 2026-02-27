// ─────────────────────────────────────────────────────────────────────────────
// Flow Templates — Curated Agent-Centric Flow Patterns
// Pure data, no DOM, no IPC. Imported by molecules/index.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowTemplate } from './atoms';

export const FLOW_TEMPLATES: FlowTemplate[] = [
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

  // ── Productivity ─────────────────────────────────────────────────────────

  {
    id: 'tpl-task-breakdown',
    name: 'Task Breakdown',
    description: 'Break a large task into subtasks with estimates and priorities.',
    category: 'productivity',
    tags: ['tasks', 'planning', 'breakdown', 'project'],
    icon: 'checklist',
    nodes: [
      { kind: 'trigger', label: 'Task Input', description: 'User describes task', config: {} },
      {
        kind: 'agent',
        label: 'Analyze Scope',
        description: 'Understand requirements',
        config: {
          prompt:
            'Analyze the task: identify the scope, dependencies, and any ambiguities that need clarification.',
        },
      },
      {
        kind: 'agent',
        label: 'Break Down',
        description: 'Create subtasks',
        config: {
          prompt:
            'Break this into actionable subtasks. For each: title, description, estimated time, priority (high/medium/low), dependencies.',
        },
      },
      {
        kind: 'output',
        label: 'Task Board',
        description: 'Add to Kanban',
        config: { outputTarget: 'store' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
      { fromIdx: 2, toIdx: 3 },
    ],
  },

  {
    id: 'tpl-standup-report',
    name: 'Standup Report',
    description: 'Generate daily standup updates from recent activity.',
    category: 'productivity',
    tags: ['standup', 'daily', 'status', 'report'],
    icon: 'assignment',
    nodes: [
      {
        kind: 'trigger',
        label: 'Morning Trigger',
        description: 'Cron: 9:00 AM',
        config: { prompt: 'Run every weekday morning' },
      },
      {
        kind: 'tool',
        label: 'Fetch Activity',
        description: 'Git, tasks, messages',
        config: {
          prompt:
            'Gather recent commits, completed tasks, and channel messages from the last 24 hours',
        },
      },
      {
        kind: 'agent',
        label: 'Generate Report',
        description: 'Format standup',
        config: {
          prompt:
            'Generate a standup report: Yesterday (completed), Today (planned), Blockers. Keep it concise.',
        },
      },
      {
        kind: 'output',
        label: 'Post',
        description: 'Send to team channel',
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
    id: 'tpl-doc-generator',
    name: 'Documentation Generator',
    description: 'Auto-generate docs from code, configuration, or specifications.',
    category: 'productivity',
    tags: ['docs', 'documentation', 'code', 'generate'],
    icon: 'description',
    nodes: [
      { kind: 'trigger', label: 'Source Input', description: 'Code or spec file', config: {} },
      {
        kind: 'agent',
        label: 'Analyze Structure',
        description: 'Parse components',
        config: {
          prompt:
            'Analyze the source code/spec. Identify modules, functions, types, dependencies, and overall architecture.',
        },
      },
      {
        kind: 'agent',
        label: 'Write Docs',
        description: 'Generate documentation',
        config: {
          prompt:
            'Write clear, comprehensive documentation. Include: overview, API reference, usage examples, configuration options.',
        },
      },
      {
        kind: 'output',
        label: 'Save Docs',
        description: 'Write to file',
        config: { outputTarget: 'store' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
      { fromIdx: 2, toIdx: 3 },
    ],
  },

  // ── Data & Transform ─────────────────────────────────────────────────────

  {
    id: 'tpl-csv-analyzer',
    name: 'CSV Analyzer',
    description: 'Upload CSV data, analyze patterns, generate insights.',
    category: 'data',
    tags: ['csv', 'data', 'analysis', 'insights'],
    icon: 'table_chart',
    nodes: [
      { kind: 'trigger', label: 'CSV Upload', description: 'File input', config: {} },
      {
        kind: 'data',
        label: 'Parse CSV',
        description: 'Extract rows',
        config: {
          transform:
            'Parse the CSV into structured data. Handle headers, types, and missing values.',
        },
      },
      {
        kind: 'agent',
        label: 'Analyze',
        description: 'Find patterns',
        config: {
          prompt:
            'Analyze the data: identify trends, outliers, correlations, and key statistics. Provide actionable insights.',
        },
      },
      {
        kind: 'output',
        label: 'Report',
        description: 'Summary with charts',
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
    id: 'tpl-api-etl',
    name: 'API Data Pipeline',
    description: 'Extract from API, transform data, load to destination.',
    category: 'data',
    tags: ['api', 'etl', 'pipeline', 'transform'],
    icon: 'api',
    nodes: [
      {
        kind: 'trigger',
        label: 'Schedule',
        description: 'Periodic fetch',
        config: { prompt: 'Run on a schedule (e.g., every hour)' },
      },
      {
        kind: 'tool',
        label: 'Fetch API',
        description: 'GET /data',
        config: { prompt: 'Make an HTTP GET request to the source API endpoint' },
      },
      {
        kind: 'data',
        label: 'Transform',
        description: 'Map & filter',
        config: {
          transform:
            'Transform: filter out inactive records, map fields to destination schema, validate required fields.',
        },
      },
      {
        kind: 'condition',
        label: 'Has Data?',
        config: { conditionExpr: 'Was valid data returned from the API?' },
      },
      {
        kind: 'tool',
        label: 'Store Data',
        description: 'Write to destination',
        config: { prompt: 'Store the transformed data in the destination' },
      },
      {
        kind: 'output',
        label: 'No Data',
        description: 'Empty response',
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

  // ── Research ─────────────────────────────────────────────────────────────

  {
    id: 'tpl-deep-research',
    name: 'Deep Research',
    description: 'Multi-source research pipeline with synthesis and citation.',
    category: 'research',
    tags: ['research', 'web', 'synthesis', 'analysis'],
    icon: 'science',
    nodes: [
      { kind: 'trigger', label: 'Research Query', description: 'User question', config: {} },
      {
        kind: 'agent',
        label: 'Plan Research',
        description: 'Define search strategy',
        config: {
          prompt:
            'Break the research question into 3-5 specific sub-queries to search. Identify key terms and angles.',
        },
      },
      {
        kind: 'tool',
        label: 'Web Search',
        description: 'Multi-engine search',
        config: { prompt: 'Search the web using the planned sub-queries. Gather diverse sources.' },
      },
      {
        kind: 'agent',
        label: 'Analyze Sources',
        description: 'Extract & verify',
        config: {
          prompt:
            'Analyze each source: extract key claims, check consistency across sources, flag contradictions.',
        },
      },
      {
        kind: 'agent',
        label: 'Synthesize',
        description: 'Final report',
        config: {
          prompt:
            'Synthesize all findings into a comprehensive report with citations. Include confidence levels and areas needing further research.',
        },
      },
      {
        kind: 'output',
        label: 'Report',
        description: 'Research output',
        config: { outputTarget: 'chat' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
      { fromIdx: 2, toIdx: 3 },
      { fromIdx: 3, toIdx: 4 },
      { fromIdx: 4, toIdx: 5 },
    ],
  },

  {
    id: 'tpl-competitor-analysis',
    name: 'Competitor Analysis',
    description: 'Research competitors, compare features, identify opportunities.',
    category: 'research',
    tags: ['competitor', 'market', 'comparison', 'strategy'],
    icon: 'insights',
    nodes: [
      { kind: 'trigger', label: 'Competitors', description: 'List of competitors', config: {} },
      {
        kind: 'tool',
        label: 'Scrape Websites',
        description: 'Gather public info',
        config: {
          prompt:
            'Fetch public information from each competitor website: features, pricing, positioning.',
        },
      },
      {
        kind: 'agent',
        label: 'Compare Features',
        description: 'Feature matrix',
        config: {
          prompt:
            'Create a feature comparison matrix. Identify strengths, weaknesses, and unique differentiators for each.',
        },
      },
      {
        kind: 'agent',
        label: 'Find Gaps',
        description: 'Opportunity analysis',
        config: {
          prompt:
            'Identify gaps in the market that no competitor covers well. Suggest opportunities for differentiation.',
        },
      },
      {
        kind: 'output',
        label: 'Report',
        description: 'Strategy brief',
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

  // ── Social & Content ─────────────────────────────────────────────────────

  {
    id: 'tpl-content-pipeline',
    name: 'Content Pipeline',
    description: 'Create, review, and publish content across platforms.',
    category: 'social',
    tags: ['content', 'blog', 'social', 'publish'],
    icon: 'edit_note',
    nodes: [
      { kind: 'trigger', label: 'Topic', description: 'Content brief', config: {} },
      {
        kind: 'agent',
        label: 'Research',
        description: 'Gather material',
        config: {
          prompt:
            'Research the topic: find key points, statistics, examples, and relevant references.',
        },
      },
      {
        kind: 'agent',
        label: 'Write Draft',
        description: 'First draft',
        config: {
          prompt:
            'Write a compelling first draft. Include hook, body, CTA. Match the target platform format.',
        },
      },
      {
        kind: 'agent',
        label: 'Edit & Polish',
        description: 'Review quality',
        config: {
          prompt: 'Edit for clarity, grammar, tone. Optimize for engagement and readability.',
        },
      },
      {
        kind: 'condition',
        label: 'Approved?',
        config: { conditionExpr: 'Does the content meet quality standards?' },
      },
      {
        kind: 'output',
        label: 'Publish',
        description: 'Post to platform',
        config: { outputTarget: 'chat' },
      },
      {
        kind: 'output',
        label: 'Needs Revision',
        description: 'Feedback loop',
        config: { outputTarget: 'chat' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
      { fromIdx: 2, toIdx: 3 },
      { fromIdx: 3, toIdx: 4 },
      { fromIdx: 4, toIdx: 5, label: 'Yes' },
      { fromIdx: 4, toIdx: 6, label: 'No' },
    ],
  },

  {
    id: 'tpl-social-monitor',
    name: 'Social Monitor',
    description: 'Monitor social mentions, analyze sentiment, alert on negative trends.',
    category: 'social',
    tags: ['social', 'monitor', 'sentiment', 'alert'],
    icon: 'monitoring',
    nodes: [
      {
        kind: 'trigger',
        label: 'Scheduled Check',
        description: 'Every 2 hours',
        config: { prompt: 'Run every 2 hours during business hours' },
      },
      {
        kind: 'tool',
        label: 'Fetch Mentions',
        description: 'Social APIs',
        config: { prompt: 'Search for brand mentions across social platforms' },
      },
      {
        kind: 'agent',
        label: 'Analyze Sentiment',
        description: 'Classify tone',
        config: {
          prompt:
            'Analyze sentiment of each mention: positive, neutral, or negative. Flag urgent issues.',
        },
      },
      {
        kind: 'condition',
        label: 'Negative Trend?',
        config: { conditionExpr: 'Is there a significant negative sentiment trend?' },
      },
      {
        kind: 'output',
        label: 'Alert Team',
        description: 'Urgent notification',
        config: { outputTarget: 'chat' },
      },
      {
        kind: 'output',
        label: 'Log Report',
        description: 'Store for review',
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

  // ── Finance & Trading ────────────────────────────────────────────────────

  {
    id: 'tpl-price-alert',
    name: 'Price Alert',
    description: 'Monitor asset prices, trigger alerts on thresholds.',
    category: 'finance',
    tags: ['trading', 'price', 'alert', 'crypto', 'defi'],
    icon: 'candlestick_chart',
    nodes: [
      {
        kind: 'trigger',
        label: 'Price Check',
        description: 'Every 5 minutes',
        config: { prompt: 'Check prices every 5 minutes' },
      },
      {
        kind: 'tool',
        label: 'Fetch Prices',
        description: 'API call',
        config: { prompt: 'Fetch current prices for monitored assets' },
      },
      {
        kind: 'condition',
        label: 'Threshold Hit?',
        config: { conditionExpr: 'Has any asset crossed a price threshold?' },
      },
      {
        kind: 'agent',
        label: 'Analyze Move',
        description: 'Context analysis',
        config: {
          prompt:
            'Analyze the price movement: recent trend, volume, potential cause. Assess if action is warranted.',
        },
      },
      {
        kind: 'output',
        label: 'Alert',
        description: 'Notification',
        config: { outputTarget: 'chat' },
      },
      {
        kind: 'output',
        label: 'Normal',
        description: 'No action',
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
    id: 'tpl-portfolio-report',
    name: 'Portfolio Report',
    description: 'Daily portfolio summary with P&L, allocation, and risk assessment.',
    category: 'finance',
    tags: ['portfolio', 'report', 'trading', 'risk'],
    icon: 'account_balance',
    nodes: [
      {
        kind: 'trigger',
        label: 'Daily Close',
        description: 'End of day',
        config: { prompt: 'Run at market close daily' },
      },
      {
        kind: 'tool',
        label: 'Fetch Positions',
        description: 'Get all holdings',
        config: {
          prompt: 'Fetch all current positions, balances, and transaction history for the day',
        },
      },
      {
        kind: 'data',
        label: 'Calculate P&L',
        description: 'Profit & loss',
        config: {
          transform:
            'Calculate daily P&L, overall returns, allocation percentages, and exposure levels.',
        },
      },
      {
        kind: 'agent',
        label: 'Risk Assessment',
        description: 'Analyze risk',
        config: {
          prompt:
            'Assess portfolio risk: concentration risk, volatility exposure, correlation analysis. Flag any positions exceeding limits.',
        },
      },
      {
        kind: 'output',
        label: 'Report',
        description: 'Daily summary',
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

  // ── Support ──────────────────────────────────────────────────────────────

  {
    id: 'tpl-support-triage',
    name: 'Support Triage',
    description: 'Classify support tickets, route to the right agent or team.',
    category: 'support',
    tags: ['support', 'tickets', 'triage', 'routing'],
    icon: 'support_agent',
    nodes: [
      { kind: 'trigger', label: 'New Ticket', description: 'Incoming request', config: {} },
      {
        kind: 'agent',
        label: 'Classify',
        description: 'Category & priority',
        config: {
          prompt:
            'Classify the support ticket: category (billing, technical, feature request, bug), priority (urgent, high, normal, low).',
        },
      },
      {
        kind: 'condition',
        label: 'Auto-Resolvable?',
        config: { conditionExpr: 'Can this be resolved with existing documentation or FAQ?' },
      },
      {
        kind: 'agent',
        label: 'Auto-Resolve',
        description: 'Generate response',
        config: {
          prompt:
            'Generate a helpful response using the knowledge base. Include relevant docs/links.',
        },
      },
      {
        kind: 'output',
        label: 'Send Response',
        description: 'Auto-reply',
        config: { outputTarget: 'chat' },
      },
      {
        kind: 'output',
        label: 'Escalate',
        description: 'Route to human',
        config: { outputTarget: 'chat' },
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
    id: 'tpl-feedback-analyzer',
    name: 'Feedback Analyzer',
    description: 'Collect user feedback, categorize themes, generate improvement suggestions.',
    category: 'support',
    tags: ['feedback', 'analysis', 'improvement', 'ux'],
    icon: 'rate_review',
    nodes: [
      {
        kind: 'trigger',
        label: 'Feedback Batch',
        description: 'Weekly collection',
        config: { prompt: 'Collect feedback from the past week' },
      },
      {
        kind: 'data',
        label: 'Aggregate',
        description: 'Combine sources',
        config: {
          transform:
            'Aggregate feedback from all sources: surveys, support tickets, social mentions, in-app feedback.',
        },
      },
      {
        kind: 'agent',
        label: 'Categorize',
        description: 'Theme extraction',
        config: {
          prompt:
            'Categorize feedback into themes. Identify recurring patterns, pain points, and feature requests. Quantify by frequency.',
        },
      },
      {
        kind: 'agent',
        label: 'Recommendations',
        description: 'Action items',
        config: {
          prompt:
            'Based on the themes, recommend specific improvements. Prioritize by impact and effort. Include supporting quotes.',
        },
      },
      {
        kind: 'output',
        label: 'Report',
        description: 'Feedback report',
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

  // ── More AI Patterns ─────────────────────────────────────────────────────

  {
    id: 'tpl-rag-pipeline',
    name: 'RAG Pipeline',
    description: 'Retrieval-augmented generation: search memory, augment context, generate.',
    category: 'ai',
    tags: ['rag', 'memory', 'retrieval', 'augmented'],
    icon: 'search_insights',
    nodes: [
      { kind: 'trigger', label: 'User Query', description: 'Incoming question', config: {} },
      {
        kind: 'tool',
        label: 'Search Memory',
        description: 'Vector + BM25',
        config: { prompt: 'Search the memory palace for relevant context using the query' },
      },
      {
        kind: 'data',
        label: 'Rank & Filter',
        description: 'MMR re-ranking',
        config: {
          transform:
            'Re-rank results by relevance. Filter to top 5 most relevant chunks. Deduplicate.',
        },
      },
      {
        kind: 'agent',
        label: 'Generate Answer',
        description: 'Augmented response',
        config: {
          prompt:
            'Answer the query using the retrieved context. Cite specific memory entries. If context is insufficient, say so.',
        },
      },
      {
        kind: 'output',
        label: 'Response',
        description: 'With citations',
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

  {
    id: 'tpl-skill-builder',
    name: 'Skill Builder',
    description: 'Agent creates, tests, and installs new skills automatically.',
    category: 'ai',
    tags: ['skills', 'skill', 'generate', 'foundry'],
    icon: 'build',
    nodes: [
      {
        kind: 'trigger',
        label: 'Skill Request',
        description: 'User describes capability',
        config: {},
      },
      {
        kind: 'agent',
        label: 'Design Skill',
        description: 'Plan SKILL.md',
        config: {
          prompt:
            'Design the skill: identify required tools, credentials, instructions, and example prompts. Plan the SKILL.md structure.',
        },
      },
      {
        kind: 'agent',
        label: 'Write Skill',
        description: 'Generate SKILL.md',
        config: {
          prompt:
            'Write the complete SKILL.md file with clear instructions, tool definitions, and usage examples.',
        },
      },
      {
        kind: 'agent',
        label: 'Test Skill',
        description: 'Dry run',
        config: {
          prompt:
            'Test the skill by running through the example prompts. Verify the instructions are clear and the tools work.',
        },
      },
      {
        kind: 'condition',
        label: 'Tests Pass?',
        config: { conditionExpr: 'Did the skill tests pass?' },
      },
      {
        kind: 'output',
        label: 'Install',
        description: 'Save to skills/',
        config: { outputTarget: 'store' },
      },
      {
        kind: 'output',
        label: 'Fix Issues',
        description: 'Iterate',
        config: { outputTarget: 'chat' },
      },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
      { fromIdx: 2, toIdx: 3 },
      { fromIdx: 3, toIdx: 4 },
      { fromIdx: 4, toIdx: 5, label: 'Pass' },
      { fromIdx: 4, toIdx: 6, label: 'Fail' },
    ],
  },

  {
    id: 'tpl-translation-pipeline',
    name: 'Translation Pipeline',
    description: 'Translate content through multiple passes for quality.',
    category: 'ai',
    tags: ['translation', 'i18n', 'language', 'localization'],
    icon: 'translate',
    nodes: [
      { kind: 'trigger', label: 'Source Text', description: 'Input content', config: {} },
      {
        kind: 'agent',
        label: 'Translate',
        description: 'First pass',
        config: {
          prompt:
            'Translate the text to the target language. Preserve meaning, tone, and technical terms.',
        },
      },
      {
        kind: 'agent',
        label: 'Back-Translate',
        description: 'Quality check',
        config: {
          prompt: 'Translate back to the original language. This is for quality verification.',
        },
      },
      {
        kind: 'agent',
        label: 'Compare & Fix',
        description: 'Resolve differences',
        config: {
          prompt:
            'Compare original and back-translation. Fix any semantic drift in the target translation.',
        },
      },
      {
        kind: 'output',
        label: 'Final Translation',
        description: 'Verified output',
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
];
