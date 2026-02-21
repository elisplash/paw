import { describe, it, expect } from 'vitest';
import { isChannelConfigured, emptyChannelConfig, CHANNEL_CLASSES, CHANNEL_SETUPS } from './atoms';

// ── isChannelConfigured ────────────────────────────────────────────────

describe('isChannelConfigured', () => {
  it('discord: configured with bot_token', () => {
    expect(isChannelConfigured('discord', { bot_token: 'abc' })).toBe(true);
  });

  it('discord: not configured without bot_token', () => {
    expect(isChannelConfigured('discord', {})).toBe(false);
  });

  it('irc: needs server and nick', () => {
    expect(isChannelConfigured('irc', { server: 'irc.libera.chat', nick: 'bot' })).toBe(true);
    expect(isChannelConfigured('irc', { server: 'irc.libera.chat' })).toBe(false);
  });

  it('slack: needs both tokens', () => {
    expect(isChannelConfigured('slack', { bot_token: 'xoxb-...', app_token: 'xapp-...' })).toBe(
      true,
    );
    expect(isChannelConfigured('slack', { bot_token: 'xoxb-...' })).toBe(false);
  });

  it('matrix: needs homeserver and access_token', () => {
    expect(
      isChannelConfigured('matrix', { homeserver: 'https://matrix.org', access_token: 'syt_...' }),
    ).toBe(true);
  });

  it('whatsapp: configured when enabled', () => {
    expect(isChannelConfigured('whatsapp', { enabled: true })).toBe(true);
    expect(isChannelConfigured('whatsapp', {})).toBe(false);
  });

  it('returns false for unknown channels', () => {
    expect(isChannelConfigured('unknown', {})).toBe(false);
  });
});

// ── emptyChannelConfig ─────────────────────────────────────────────────

describe('emptyChannelConfig', () => {
  it('returns base config for discord', () => {
    const config = emptyChannelConfig('discord');
    expect(config.enabled).toBe(false);
    expect(config.bot_token).toBe('');
    expect(config.respond_to_mentions).toBe(true);
  });

  it('returns base config for irc', () => {
    const config = emptyChannelConfig('irc');
    expect(config.server).toBe('');
    expect(config.port).toBe(6697);
    expect(config.tls).toBe(true);
  });

  it('returns generic base for unknown', () => {
    const config = emptyChannelConfig('unknown_channel');
    expect(config.enabled).toBe(false);
    expect(config.dm_policy).toBe('pairing');
  });
});

// ── CHANNEL_CLASSES ────────────────────────────────────────────────────

describe('CHANNEL_CLASSES', () => {
  it('contains all expected channels', () => {
    expect(Object.keys(CHANNEL_CLASSES)).toEqual(
      expect.arrayContaining(['telegram', 'discord', 'slack', 'matrix', 'whatsapp']),
    );
  });
});

// ── CHANNEL_SETUPS ─────────────────────────────────────────────────────

describe('CHANNEL_SETUPS', () => {
  it('has definitions for all channels', () => {
    const ids = CHANNEL_SETUPS.map((s) => s.id);
    expect(ids).toContain('telegram');
    expect(ids).toContain('discord');
    expect(ids).toContain('webchat');
  });

  it('each setup has required fields', () => {
    for (const setup of CHANNEL_SETUPS) {
      expect(setup.id).toBeTruthy();
      expect(setup.name).toBeTruthy();
      expect(setup.description).toBeTruthy();
      expect(Array.isArray(setup.fields)).toBe(true);
      expect(typeof setup.buildConfig).toBe('function');
    }
  });
});
