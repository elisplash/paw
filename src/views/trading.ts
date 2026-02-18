// Trading Dashboard — Portfolio, P&L, Trade History, Auto-Trade Policy
// Visual representation of Coinbase + DEX trading activity and automated guidelines.

import { pawEngine, type TradeRecord, type TradingSummary, type TradingPolicy } from '../engine';

const $ = (id: string) => document.getElementById(id);

// ── Module state ───────────────────────────────────────────────────────────
let wsConnected = false;

export function setWsConnected(connected: boolean) {
  wsConnected = connected;
  // Wire refresh button
  const refreshBtn = $('trading-refresh');
  if (refreshBtn && !refreshBtn.dataset.bound) {
    refreshBtn.dataset.bound = '1';
    refreshBtn.addEventListener('click', () => loadTrading());
  }
}

// ── Helpers ────────────────────────────────────────────────────────────────
function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

function formatUsd(value: number | string | null): string {
  if (value === null || value === undefined) return '$0.00';
  const num = typeof value === 'string' ? parseFloat(value) : value;
  if (isNaN(num)) return '$0.00';
  return num < 0 ? `-$${Math.abs(num).toFixed(2)}` : `$${num.toFixed(2)}`;
}

function formatAmount(value: string | number): string {
  const num = typeof value === 'string' ? parseFloat(value) : value;
  if (isNaN(num)) return String(value);
  if (num >= 1_000_000_000) return `${(num / 1_000_000_000).toFixed(2)}B`;
  if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(2)}M`;
  if (num >= 10_000) return `${(num / 1_000).toFixed(1)}K`;
  if (num < 0.0001 && num > 0) return num.toExponential(2);
  if (num >= 100) return num.toFixed(2);
  return num.toPrecision(6);
}

function formatTime(isoStr: string): string {
  try {
    const d = new Date(isoStr + (isoStr.includes('Z') ? '' : 'Z'));
    return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  } catch { return isoStr; }
}

function pnlClass(value: number): string {
  if (value > 0) return 'trading-positive';
  if (value < 0) return 'trading-negative';
  return 'trading-neutral';
}

function tradeTypeLabel(t: TradeRecord): string {
  if (t.trade_type === 'dex_swap') return 'DEX_SWAP';
  if (t.trade_type === 'transfer') return 'TRANSFER';
  return 'TRADE';
}

function tradeSideLabel(t: TradeRecord): string {
  if (t.trade_type === 'dex_swap') return 'swap';
  return t.side || '-';
}

function tradePairLabel(t: TradeRecord): string {
  if (t.trade_type === 'dex_swap' && t.product_id) return t.product_id;
  if (t.trade_type === 'dex_swap' && t.currency) {
    const out = t.to_address && !t.to_address.startsWith('0x') ? t.to_address : '?';
    return `${t.currency.toUpperCase()} → ${out.toUpperCase()}`;
  }
  return t.product_id || t.currency || '-';
}

// ── Main Loader ────────────────────────────────────────────────────────────
export async function loadTrading() {
  if (!wsConnected) return;

  const container = $('trading-content');
  if (!container) return;

  try {
    const [trades, summary, policy] = await Promise.all([
      pawEngine.tradingHistory(100),
      pawEngine.tradingSummary(),
      pawEngine.tradingPolicyGet(),
    ]);

    renderDashboard(container, trades, summary, policy);
  } catch (err) {
    container.innerHTML = `<div class="trading-error">Failed to load trading data: ${escHtml(String(err))}</div>`;
  }
}

// ── Render Dashboard ───────────────────────────────────────────────────────
function renderDashboard(
  container: HTMLElement,
  trades: TradeRecord[],
  summary: TradingSummary,
  policy: TradingPolicy,
) {
  const totalOps = summary.trade_count + summary.transfer_count + summary.dex_swap_count;
  const hasDex = summary.dex_swap_count > 0;

  // Recent activity streak
  const recentTrades = trades.slice(0, 5);
  const latestStatus = recentTrades.length > 0 ? recentTrades[0].status : 'none';
  const streakEmoji = latestStatus === 'completed' ? '🟢' : latestStatus === 'pending' ? '🟡' : '⚪';

  container.innerHTML = `
    <!-- Summary Cards -->
    <div class="trading-cards">
      <div class="trading-card ${pnlClass(summary.net_pnl_usd)}">
        <div class="trading-card-label">Today's P&L</div>
        <div class="trading-card-value ${pnlClass(summary.net_pnl_usd)}">
          ${formatUsd(summary.net_pnl_usd)}
        </div>
        ${summary.net_pnl_usd !== 0 ? `<div class="trading-card-sub">${summary.net_pnl_usd > 0 ? '↑' : '↓'} ${Math.abs(summary.net_pnl_usd).toFixed(2)} USD</div>` : ''}
      </div>
      <div class="trading-card">
        <div class="trading-card-label">Bought Today</div>
        <div class="trading-card-value">${formatUsd(summary.buy_total_usd)}</div>
        ${summary.trade_count > 0 ? `<div class="trading-card-sub">${summary.trade_count} trade${summary.trade_count > 1 ? 's' : ''}</div>` : ''}
      </div>
      <div class="trading-card">
        <div class="trading-card-label">Sold Today</div>
        <div class="trading-card-value">${formatUsd(summary.sell_total_usd)}</div>
      </div>
      <div class="trading-card">
        <div class="trading-card-label">Transfers</div>
        <div class="trading-card-value">${summary.transfer_count > 0 ? formatUsd(summary.transfer_total_usd) : summary.transfer_count.toString()}</div>
        ${summary.transfer_count > 0 ? `<div class="trading-card-sub">${summary.transfer_count} transfer${summary.transfer_count > 1 ? 's' : ''}</div>` : ''}
      </div>
      <div class="trading-card trading-card-accent">
        <div class="trading-card-label">DEX Swaps</div>
        <div class="trading-card-value">${summary.dex_swap_count}</div>
        ${hasDex ? `<div class="trading-card-sub">${formatAmount(summary.dex_volume_raw)} tokens swapped</div>` : ''}
      </div>
      <div class="trading-card">
        <div class="trading-card-label">Total Operations</div>
        <div class="trading-card-value">${streakEmoji} ${totalOps}</div>
        <div class="trading-card-sub">${latestStatus === 'completed' ? 'All systems go' : latestStatus === 'pending' ? 'Pending...' : 'Idle'}</div>
      </div>
      <div class="trading-card">
        <div class="trading-card-label">Daily Spent</div>
        <div class="trading-card-value">${formatUsd(summary.daily_spent_usd)} / ${formatUsd(policy.max_daily_loss_usd)}</div>
        <div class="trading-card-bar">
          <div class="trading-card-bar-fill ${summary.daily_spent_usd > policy.max_daily_loss_usd * 0.8 ? 'warn' : ''}"
               style="width: ${Math.min(100, (summary.daily_spent_usd / Math.max(1, policy.max_daily_loss_usd)) * 100)}%"></div>
        </div>
      </div>
    </div>

    ${hasDex && summary.dex_pairs && summary.dex_pairs.length > 0 ? `
    <!-- Active DEX Pairs -->
    <div class="trading-dex-pairs">
      <span class="trading-dex-pairs-label">Active pairs:</span>
      ${summary.dex_pairs.map((p: string) => `<span class="trading-dex-pair-tag">${escHtml(p)}</span>`).join('')}
    </div>
    ` : ''}

    <!-- Auto-Trade Policy -->
    <div class="trading-section">
      <div class="trading-section-header">
        <h3>Auto-Trade Policy</h3>
        <div class="trading-policy-toggle">
          <label class="trading-toggle-label">
            <input type="checkbox" id="trading-auto-approve" ${policy.auto_approve ? 'checked' : ''}>
            <span>Auto-approve trades within guidelines</span>
          </label>
        </div>
      </div>
      <div class="trading-policy-grid" id="trading-policy-fields" style="${policy.auto_approve ? '' : 'opacity: 0.5; pointer-events: none;'}">
        <div class="trading-policy-field">
          <label>Max Trade (USD)</label>
          <input type="number" id="trading-max-trade" value="${policy.max_trade_usd}" min="0" step="10">
        </div>
        <div class="trading-policy-field">
          <label>Max Daily Spend (USD)</label>
          <input type="number" id="trading-max-daily" value="${policy.max_daily_loss_usd}" min="0" step="50">
        </div>
        <div class="trading-policy-field">
          <label>Allowed Pairs</label>
          <input type="text" id="trading-allowed-pairs" value="${escHtml(policy.allowed_pairs.join(', '))}" placeholder="BTC-USD, ETH-USD (empty = all)">
        </div>
        <div class="trading-policy-field">
          <label class="trading-toggle-label">
            <input type="checkbox" id="trading-allow-transfers" ${policy.allow_transfers ? 'checked' : ''}>
            <span>Allow auto-approve transfers</span>
          </label>
        </div>
        <div class="trading-policy-field">
          <label>Max Transfer (USD)</label>
          <input type="number" id="trading-max-transfer" value="${policy.max_transfer_usd}" min="0" step="10">
        </div>
        <div class="trading-policy-field trading-policy-actions">
          <button class="btn-primary" id="trading-save-policy">Save Policy</button>
        </div>
      </div>
    </div>

    <!-- Trade History -->
    <div class="trading-section">
      <div class="trading-section-header">
        <h3>Trade History</h3>
        <span class="trading-history-count">${trades.length} record${trades.length !== 1 ? 's' : ''}</span>
      </div>
      ${trades.length === 0
        ? '<div class="trading-empty">No trades recorded yet. Your agents\' trades and swaps will appear here.</div>'
        : `<div class="trading-table-wrap">
            <table class="trading-table">
              <thead>
                <tr>
                  <th>Time</th>
                  <th>Type</th>
                  <th>Side</th>
                  <th>Pair / Token</th>
                  <th>Amount</th>
                  <th>Status</th>
                  <th>Reason</th>
                </tr>
              </thead>
              <tbody>
                ${trades.map(t => {
                  const sideClass = t.side === 'buy' ? 'trading-positive'
                    : t.side === 'sell' ? 'trading-negative'
                    : t.trade_type === 'dex_swap' ? 'trading-swap'
                    : '';
                  return `
                  <tr class="trading-row ${t.trade_type}">
                    <td class="trading-time">${formatTime(t.created_at)}</td>
                    <td><span class="trading-badge ${t.trade_type}">${tradeTypeLabel(t)}</span></td>
                    <td class="${sideClass}">${escHtml(tradeSideLabel(t))}</td>
                    <td class="trading-pair">${escHtml(tradePairLabel(t))}</td>
                    <td class="trading-amount">${formatAmount(t.amount)}${t.usd_value ? ` <span class="trading-usd">(${formatUsd(t.usd_value)})</span>` : ''}</td>
                    <td><span class="trading-status ${t.status}">${escHtml(t.status)}</span></td>
                    <td class="trading-reason" title="${escHtml(t.reason || '')}">${escHtml(t.reason || '-')}</td>
                  </tr>`;
                }).join('')}
              </tbody>
            </table>
          </div>`
      }
    </div>
  `;

  // Wire up event listeners
  bindPolicyEvents();
}

// ── Policy Form Events ─────────────────────────────────────────────────────
function bindPolicyEvents() {
  const autoApprove = $('trading-auto-approve') as HTMLInputElement | null;
  const fields = $('trading-policy-fields');
  const saveBtn = $('trading-save-policy');

  if (autoApprove && fields) {
    autoApprove.addEventListener('change', () => {
      fields.style.opacity = autoApprove.checked ? '1' : '0.5';
      fields.style.pointerEvents = autoApprove.checked ? '' : 'none';
    });
  }

  if (saveBtn) {
    saveBtn.addEventListener('click', async () => {
      const maxTrade = parseFloat(($('trading-max-trade') as HTMLInputElement)?.value || '100');
      const maxDaily = parseFloat(($('trading-max-daily') as HTMLInputElement)?.value || '500');
      const pairsRaw = ($('trading-allowed-pairs') as HTMLInputElement)?.value || '';
      const allowTransfers = ($('trading-allow-transfers') as HTMLInputElement)?.checked || false;
      const maxTransfer = parseFloat(($('trading-max-transfer') as HTMLInputElement)?.value || '0');
      const autoApproveChecked = ($('trading-auto-approve') as HTMLInputElement)?.checked || false;

      const pairs = pairsRaw.split(',').map(s => s.trim()).filter(Boolean);

      const policy: TradingPolicy = {
        auto_approve: autoApproveChecked,
        max_trade_usd: maxTrade,
        max_daily_loss_usd: maxDaily,
        allowed_pairs: pairs,
        allow_transfers: allowTransfers,
        max_transfer_usd: maxTransfer,
      };

      try {
        await pawEngine.tradingPolicySet(policy);
        showTradingToast('Trading policy saved', 'success');
      } catch (err) {
        showTradingToast(`Failed to save policy: ${err}`, 'error');
      }
    });
  }
}

// ── Toast ──────────────────────────────────────────────────────────────────
let _tradingToastTimer: number | null = null;
function showTradingToast(message: string, type: 'success' | 'error' | 'info') {
  const toast = $('trading-toast');
  if (!toast) return;
  toast.className = `trading-toast ${type}`;
  toast.textContent = message;
  toast.style.display = 'flex';

  if (_tradingToastTimer) clearTimeout(_tradingToastTimer);
  _tradingToastTimer = window.setTimeout(() => {
    toast.style.display = 'none';
    _tradingToastTimer = null;
  }, type === 'error' ? 8000 : 4000);
}
