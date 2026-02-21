// Trading Dashboard — Pure helpers (no DOM, no IPC)

import type { TradeRecord } from '../../engine';

export function formatUsd(value: number | string | null): string {
  if (value === null || value === undefined) return '$0.00';
  const num = typeof value === 'string' ? parseFloat(value) : value;
  if (isNaN(num)) return '$0.00';
  return num < 0 ? `-$${Math.abs(num).toFixed(2)}` : `$${num.toFixed(2)}`;
}

export function formatAmount(value: string | number): string {
  const num = typeof value === 'string' ? parseFloat(value) : value;
  if (isNaN(num)) return String(value);
  if (num >= 1_000_000_000) return `${(num / 1_000_000_000).toFixed(2)}B`;
  if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(2)}M`;
  if (num >= 10_000) return `${(num / 1_000).toFixed(1)}K`;
  if (num < 0.0001 && num > 0) return num.toExponential(2);
  if (num >= 100) return num.toFixed(2);
  return num.toPrecision(6);
}

export function formatTime(isoStr: string): string {
  try {
    const d = new Date(isoStr + (isoStr.includes('Z') ? '' : 'Z'));
    return d.toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return isoStr;
  }
}

export function formatPrice(price: number): string {
  if (price === 0) return '0';
  if (price >= 1) return price.toFixed(4);
  if (price >= 0.001) return price.toFixed(6);
  return price.toExponential(4);
}

export function pnlClass(value: number): string {
  if (value > 0) return 'trading-positive';
  if (value < 0) return 'trading-negative';
  return 'trading-neutral';
}

export function tradeTypeLabel(t: TradeRecord): string {
  if (t.trade_type === 'dex_swap') return 'DEX_SWAP';
  if (t.trade_type === 'transfer') return 'TRANSFER';
  return 'TRADE';
}

export function tradeSideLabel(t: TradeRecord): string {
  if (t.trade_type === 'dex_swap') return 'swap';
  return t.side || '-';
}

export function tradePairLabel(t: TradeRecord): string {
  if (t.trade_type === 'dex_swap' && t.product_id) return t.product_id;
  if (t.trade_type === 'dex_swap' && t.currency) {
    const out = t.to_address && !t.to_address.startsWith('0x') ? t.to_address : '?';
    return `${t.currency.toUpperCase()} → ${out.toUpperCase()}`;
  }
  return t.product_id || t.currency || '-';
}
