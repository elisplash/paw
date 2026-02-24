// helpers.ts — Internal helpers for the agents module (IPC, no DOM)
// seedSoulFiles + refreshAvailableModels

import { pawEngine } from '../../engine';
import { type Agent } from './atoms';

/**
 * Seed initial soul files for a new agent so it knows who it is from the first conversation.
 * Only writes files that don't already exist to avoid overwriting user edits.
 */
export async function seedSoulFiles(agent: Agent): Promise<void> {
  try {
    const existing = await pawEngine.agentFileList(agent.id);
    const existingNames = new Set(existing.map((f) => f.file_name));

    if (!existingNames.has('IDENTITY.md')) {
      const personality = agent.personality;
      const personalityDesc = [
        personality.tone !== 'balanced' ? `Tone: ${personality.tone}` : '',
        personality.initiative !== 'balanced' ? `Initiative: ${personality.initiative}` : '',
        personality.detail !== 'balanced' ? `Detail level: ${personality.detail}` : '',
      ]
        .filter(Boolean)
        .join(', ');

      const identity = [
        `# ${agent.name}`,
        '',
        `## Identity`,
        `- **Name**: ${agent.name}`,
        `- **Agent ID**: ${agent.id}`,
        `- **Role**: ${agent.bio || 'AI assistant'}`,
        agent.template !== 'general' && agent.template !== 'custom'
          ? `- **Specialty**: ${agent.template}`
          : '',
        personalityDesc ? `- **Personality**: ${personalityDesc}` : '',
        '',
        agent.boundaries.length > 0
          ? `## Boundaries\n${agent.boundaries.map((b) => `- ${b}`).join('\n')}`
          : '',
        '',
        agent.systemPrompt ? `## Custom Instructions\n${agent.systemPrompt}` : '',
      ]
        .filter(Boolean)
        .join('\n');

      await pawEngine.agentFileSet('IDENTITY.md', identity.trim(), agent.id);
    }

    if (!existingNames.has('SOUL.md')) {
      const soul = [
        `# Soul`,
        '',
        `Write your personality, values, and communication style here.`,
        `Use \`soul_write\` to update this file as you develop your voice.`,
      ].join('\n');
      await pawEngine.agentFileSet('SOUL.md', soul, agent.id);
    }

    if (!existingNames.has('USER.md')) {
      const user = [
        `# About the User`,
        '',
        `Record what you learn about the user here — their name, preferences, projects, etc.`,
        `Use \`soul_write\` to update this file when you learn new things.`,
      ].join('\n');
      await pawEngine.agentFileSet('USER.md', user, agent.id);
    }

    console.debug(`[agents] Seeded soul files for ${agent.name} (${agent.id})`);
  } catch (e) {
    console.warn(`[agents] Failed to seed soul files for ${agent.id}:`, e);
  }
}

/** Fetch configured models from the engine and populate the model picker. */
export async function refreshAvailableModels(): Promise<{ id: string; name: string }[]> {
  try {
    const config = await pawEngine.getConfig();
    const models: { id: string; name: string }[] = [
      { id: 'default', name: 'Default (Use account setting)' },
    ];
    // Add each provider's default model, plus well-known models per provider kind
    const WELL_KNOWN: Record<string, { id: string; name: string }[]> = {
      google: [
        { id: 'gemini-3.1-pro-preview', name: 'Gemini 3.1 Pro Preview' },
        { id: 'gemini-3-flash-preview', name: 'Gemini 3 Flash Preview' },
        { id: 'gemini-2.5-pro', name: 'Gemini 2.5 Pro' },
        { id: 'gemini-2.5-flash', name: 'Gemini 2.5 Flash' },
        { id: 'gemini-2.0-flash', name: 'Gemini 2.0 Flash' },
      ],
      anthropic: [
        { id: 'claude-sonnet-4-6', name: 'Claude Sonnet 4.6 ($3/$15)' },
        { id: 'claude-haiku-4-5-20251001', name: 'Claude Haiku 4.5 ($1/$5)' },
        { id: 'claude-3-haiku-20240307', name: 'Claude Haiku 3 ($0.25/$1.25) cheapest' },
        { id: 'claude-opus-4-6', name: 'Claude Opus 4.6 ($5/$25)' },
        { id: 'claude-sonnet-4-5-20250929', name: 'Claude Sonnet 4.5 (agentic)' },
      ],
      openai: [
        { id: 'gpt-4o', name: 'GPT-4o' },
        { id: 'gpt-4o-mini', name: 'GPT-4o Mini' },
        { id: 'o1', name: 'o1' },
        { id: 'o3-mini', name: 'o3-mini' },
      ],
      openrouter: [],
      ollama: [],
      custom: [],
    };
    const seen = new Set<string>(['default']);
    for (const p of config.providers ?? []) {
      // Provider's own default model
      if (p.default_model && !seen.has(p.default_model)) {
        seen.add(p.default_model);
        models.push({ id: p.default_model, name: `${p.default_model} (${p.kind})` });
      }
      // Well-known models for this provider kind
      for (const wk of WELL_KNOWN[p.kind] ?? []) {
        if (!seen.has(wk.id)) {
          seen.add(wk.id);
          models.push(wk);
        }
      }
    }
    // Also add the global default model if set
    if (config.default_model && !seen.has(config.default_model)) {
      models.push({ id: config.default_model, name: `${config.default_model} (default)` });
    }
    return models;
  } catch (e) {
    console.warn('[agents] Could not load models from engine config:', e);
    return [{ id: 'default', name: 'Default (Use account setting)' }];
  }
}
