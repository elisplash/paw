import { describe, it, expect } from 'vitest';
import {
  formatUsd,
  formatAmount,
  formatTime,
  formatPrice,
  pnlClass,
  tradeTypeLabel,
  tradeSideLabel,
  tradePairLabel,
} from './atoms';
import type { TradeRecord } from '../../engine';

// ── formatUsd ──────────────────────────────────────────────────────────

describe('formatUsd', () => {
  it('formats positive numbers', () => {
    expect(formatUsd(12.5)).toBe('$12.50');
    expect(formatUsd(0)).toBe('$0.00');
  });

  it('formats negative numbers with leading minus', () => {
    expect(formatUsd(-5.3)).toBe('-$5.30');
  });

  it('handles string input', () => {
    expect(formatUsd('42.1')).toBe('$42.10');
    expect(formatUsd('abc')).toBe('$0.00');
  });

  it('handles null / undefined', () => {
    expect(formatUsd(null)).toBe('$0.00');
    expect(formatUsd(undefined as unknown as null)).toBe('$0.00');
  });
});

// ── formatAmount ───────────────────────────────────────────────────────

describe('formatAmount', () => {
  it('formats billions', () => {
    expect(formatAmount(2_500_000_000)).toBe('2.50B');
  });

  it('formats millions', () => {
    expect(formatAmount(1_200_000)).toBe('1.20M');
  });

  it('formats thousands (>=10k)', () => {
    expect(formatAmount(50_000)).toBe('50.0K');
  });

  it('handles tiny positive values', () => {
    expect(formatAmount(0.00001)).toBe('1.00e-5');
  });

  it('handles >=100 with two decimals', () => {
    expect(formatAmount(150.123)).toBe('150.12');
  });

  it('handles normal values with precision 6', () => {
    expect(formatAmount(3.14159)).toBe('3.14159');
  });

  it('handles string input', () => {
    expect(formatAmount('2000000')).toBe('2.00M');
  });

  it('returns original string for NaN', () => {
    expect(formatAmount('hello')).toBe('hello');
  });
});

// ── formatTime ─────────────────────────────────────────────────────────

describe('formatTime', () => {
  it('formats ISO date string', () => {
    const result = formatTime('2024-01-15T10:30:00Z');
    expect(result).toBeTruthy();
    expect(typeof result).toBe('string');
  });

  it('returns raw string on bad input', () => {
    expect(formatTime('not-a-date')).toBeTruthy();
  });
});

// ── formatPrice ────────────────────────────────────────────────────────

describe('formatPrice', () => {
  it('returns "0" for zero', () => {
    expect(formatPrice(0)).toBe('0');
  });

  it('uses 4 decimals for >=1', () => {
    expect(formatPrice(1234.56789)).toBe('1234.5679');
  });

  it('uses 6 decimals for >=0.001', () => {
    expect(formatPrice(0.005)).toBe('0.005000');
  });

  it('uses exponential for tiny prices', () => {
    expect(formatPrice(0.0001)).toBe('1.0000e-4');
  });
});

// ── pnlClass ───────────────────────────────────────────────────────────

describe('pnlClass', () => {
  it('returns positive class for gains', () => {
    expect(pnlClass(100)).toBe('trading-positive');
  });

  it('returns negative class for losses', () => {
    expect(pnlClass(-50)).toBe('trading-negative');
  });

  it('returns neutral class for zero', () => {
    expect(pnlClass(0)).toBe('trading-neutral');
  });
});

// ── Trade label helpers ────────────────────────────────────────────────

const baseTrade: TradeRecord = {
  id: '1',
  trade_type: 'trade',
  side: 'buy',
  product_id: 'BTC-USD',
  currency: 'BTC',
  amount: '1',
  order_type: 'market',
  order_id: 'ord-1',
  status: 'filled',
  usd_value: '50000',
  to_address: null,
  reason: 'test',
  session_id: null,
  agent_id: null,
  created_at: '2024-01-01T00:00:00Z',
};

describe('tradeTypeLabel', () => {
  it('labels dex_swap', () => {
    expect(tradeTypeLabel({ ...baseTrade, trade_type: 'dex_swap' })).toBe('DEX_SWAP');
  });

  it('labels transfer', () => {
    expect(tradeTypeLabel({ ...baseTrade, trade_type: 'transfer' })).toBe('TRANSFER');
  });

  it('defaults to TRADE', () => {
    expect(tradeTypeLabel(baseTrade)).toBe('TRADE');
  });
});

describe('tradeSideLabel', () => {
  it('returns "swap" for dex_swap', () => {
    expect(tradeSideLabel({ ...baseTrade, trade_type: 'dex_swap' })).toBe('swap');
  });

  it('returns side for regular trade', () => {
    expect(tradeSideLabel({ ...baseTrade, side: 'sell' })).toBe('sell');
  });

  it('returns dash when side is null', () => {
    expect(tradeSideLabel({ ...baseTrade, side: null })).toBe('-');
  });
});

describe('tradePairLabel', () => {
  it('returns product_id for dex_swap with product_id', () => {
    expect(tradePairLabel({ ...baseTrade, trade_type: 'dex_swap', product_id: 'SOL/USDC' })).toBe(
      'SOL/USDC',
    );
  });

  it('builds label from currency for dex_swap without product_id', () => {
    const result = tradePairLabel({
      ...baseTrade,
      trade_type: 'dex_swap',
      product_id: null,
      currency: 'sol',
      to_address: 'usdc',
    });
    expect(result).toBe('SOL → USDC');
  });

  it('returns product_id for regular trade', () => {
    expect(tradePairLabel(baseTrade)).toBe('BTC-USD');
  });

  it('falls back to dash', () => {
    expect(tradePairLabel({ ...baseTrade, product_id: null, currency: null })).toBe('-');
  });
});
