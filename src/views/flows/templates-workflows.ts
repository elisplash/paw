// ─────────────────────────────────────────────────────────────────────────────
// Flow Templates — Workflows (Productivity, Data, Research, Social)
// Pure data, no DOM, no IPC.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowTemplate } from './atoms';

export const TEMPLATES_WORKFLOWS: FlowTemplate[] = [
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
];
