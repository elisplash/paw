import { describe, it, expect } from 'vitest';
import { getWeatherIcon, getGreeting, getPawzMessage, isToday } from './atoms';

// ── getWeatherIcon ─────────────────────────────────────────────────────

describe('getWeatherIcon', () => {
  it('returns sun icon for clear code 113', () => {
    expect(getWeatherIcon('113')).toContain('light_mode');
  });

  it('returns cloud icon for overcast codes', () => {
    expect(getWeatherIcon('119')).toContain('cloud');
  });

  it('returns rain icon for rain codes', () => {
    expect(getWeatherIcon('176')).toContain('rainy');
  });

  it('returns snow icon for snow codes', () => {
    expect(getWeatherIcon('179')).toContain('weather_snowy');
  });

  it('returns thunderstorm icon', () => {
    expect(getWeatherIcon('200')).toContain('thunderstorm');
  });

  it('returns default for unknown code', () => {
    expect(getWeatherIcon('999')).toContain('partly_cloudy_day');
  });
});

// ── getGreeting ────────────────────────────────────────────────────────

describe('getGreeting', () => {
  it('returns a greeting string', () => {
    const g = getGreeting();
    expect(g).toMatch(/Good (morning|afternoon|evening)/);
  });
});

// ── getPawzMessage ─────────────────────────────────────────────────────

describe('getPawzMessage', () => {
  it('mentions completed tasks when all done', () => {
    const msg = getPawzMessage(0, 5);
    expect(msg).toContain('done');
  });

  it('mentions progress when both pending and completed', () => {
    const msg = getPawzMessage(3, 2);
    expect(msg).toContain('2 down');
  });

  it('mentions pending tasks when none completed', () => {
    const msg = getPawzMessage(5, 0);
    expect(msg).toContain('5 tasks');
  });

  it('handles no tasks', () => {
    const msg = getPawzMessage(0, 0);
    expect(msg).toContain('No tasks');
  });
});

// ── isToday ────────────────────────────────────────────────────────────

describe('isToday', () => {
  it('returns true for today', () => {
    expect(isToday(new Date().toISOString())).toBe(true);
  });

  it('returns false for yesterday', () => {
    const yesterday = new Date();
    yesterday.setDate(yesterday.getDate() - 1);
    expect(isToday(yesterday.toISOString())).toBe(false);
  });
});
