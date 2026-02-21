// Trading Dashboard — DOM rendering + IPC

import {
  pawEngine,
  type TradeRecord,
  type TradingSummary,
  type TradingPolicy,
  type Position,
} from '../../engine';
import { $, escHtml, confirmModal } from '../../components/helpers';
import { isConnected } from '../../state/connection';
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

// ── Module state ───────────────────────────────────────────────────────────
let _refreshBound = false;

// ── Main Loader ────────────────────────────────────────────────────────────
export async function loadTrading() {
  if (!isConnected()) return;

  const refreshBtn = $('trading-refresh');
  if (refreshBtn && !_refreshBound) {
    _refreshBound = true;
    refreshBtn.addEventListener('click', () => loadTrading());
  }

  const container = $('trading-content');
  if (!container) return;

  try {
    const [trades, summary, policy, positions] = await Promise.all([
      pawEngine.tradingHistory(100),
      pawEngine.tradingSummary(),
      pawEngine.tradingPolicyGet(),
      pawEngine.positionsList(),
    ]);

    renderDashboard(container, trades, summary, policy, positions);
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
  positions: Position[],
) {
  const totalOps = summary.trade_count + summary.transfer_count + summary.dex_swap_count;
  const hasDex = summary.dex_swap_count > 0;

  const recentTrades = trades.slice(0, 5);
  const latestStatus = recentTrades.length > 0 ? recentTrades[0].status : 'none';
  const streakIcon =
    latestStatus === 'completed'
      ? '<span class="ms ms-sm" style="color:var(--success)">circle</span>'
      : latestStatus === 'pending'
        ? '<span class="ms ms-sm" style="color:var(--warning)">circle</span>'
        : '<span class="ms ms-sm" style="color:var(--text-muted)">circle</span>';

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
        <div class="trading-card-value">${streakIcon} ${totalOps}</div>
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

    ${
      hasDex && summary.dex_pairs && summary.dex_pairs.length > 0
        ? `
    <!-- Active DEX Pairs -->
    <div class="trading-dex-pairs">
      <span class="trading-dex-pairs-label">Active pairs:</span>
      ${summary.dex_pairs.map((p: string) => `<span class="trading-dex-pair-tag">${escHtml(p)}</span>`).join('')}
    </div>
    `
        : ''
    }

    <!-- Open Positions (Stop-Loss / Take-Profit) -->
    ${renderPositionsPanel(positions)}

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
      ${
        trades.length === 0
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
                ${trades
                  .map((t) => {
                    const sideClass =
                      t.side === 'buy'
                        ? 'trading-positive'
                        : t.side === 'sell'
                          ? 'trading-negative'
                          : t.trade_type === 'dex_swap'
                            ? 'trading-swap'
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
                  })
                  .join('')}
              </tbody>
            </table>
          </div>`
      }
    </div>
  `;

  bindPolicyEvents();
  bindPositionEvents();
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

      const pairs = pairsRaw
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean);

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

// ── Positions Panel ────────────────────────────────────────────────────────
function renderPositionsPanel(positions: Position[]): string {
  const openPositions = positions.filter((p) => p.status === 'open');
  const closedPositions = positions.filter((p) => p.status !== 'open').slice(0, 10);

  if (positions.length === 0) {
    return `
    <div class="trading-section">
      <div class="trading-section-header">
        <h3>Positions</h3>
        <span class="trading-history-count">Stop-Loss &amp; Take-Profit</span>
      </div>
      <div class="trading-empty">No positions yet. When your agent buys a token, a position with stop-loss and take-profit will be created automatically.</div>
    </div>`;
  }

  return `
  <div class="trading-section">
    <div class="trading-section-header">
      <h3>Open Positions (${openPositions.length})</h3>
      <span class="trading-history-count">Auto-managed stop-loss &amp; take-profit</span>
    </div>
    ${
      openPositions.length === 0
        ? '<div class="trading-empty">No open positions. Closed positions shown below.</div>'
        : `<div class="trading-table-wrap">
          <table class="trading-table">
            <thead>
              <tr>
                <th>Token</th>
                <th>Entry Price</th>
                <th>Current Price</th>
                <th>P&L</th>
                <th>Amount</th>
                <th>SOL In</th>
                <th>Stop-Loss</th>
                <th>Take-Profit</th>
                <th>Last Check</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              ${openPositions
                .map((p) => {
                  const pnlPct =
                    p.entry_price_usd > 0 ? (p.last_price_usd / p.entry_price_usd - 1) * 100 : 0;
                  const pnlCls =
                    pnlPct > 0
                      ? 'trading-positive'
                      : pnlPct < 0
                        ? 'trading-negative'
                        : 'trading-neutral';
                  const pnlSign = pnlPct >= 0 ? '+' : '';
                  return `
                <tr class="trading-row position-row" data-id="${escHtml(p.id)}">
                  <td class="trading-pair"><strong>${escHtml(p.symbol)}</strong><br><span class="trading-usd">${escHtml(p.mint.slice(0, 8))}…</span></td>
                  <td>$${formatPrice(p.entry_price_usd)}</td>
                  <td>$${formatPrice(p.last_price_usd)}</td>
                  <td class="${pnlCls}"><strong>${pnlSign}${pnlPct.toFixed(1)}%</strong></td>
                  <td class="trading-amount">${formatAmount(p.current_amount)}</td>
                  <td>${p.entry_sol.toFixed(4)}</td>
                  <td>${(p.stop_loss_pct * 100).toFixed(0)}%</td>
                  <td>${p.take_profit_pct.toFixed(1)}x</td>
                  <td class="trading-time">${p.last_checked_at ? formatTime(p.last_checked_at) : '—'}</td>
                  <td>
                    <button class="btn-sm btn-danger pos-close-btn" data-id="${escHtml(p.id)}" data-symbol="${escHtml(p.symbol)}">Close</button>
                  </td>
                </tr>`;
                })
                .join('')}
            </tbody>
          </table>
        </div>`
    }
    ${
      closedPositions.length > 0
        ? `
      <div style="margin-top: 12px; opacity: 0.7;">
        <details>
          <summary style="cursor: pointer; font-size: 0.85em;">Closed positions (${closedPositions.length})</summary>
          <div class="trading-table-wrap" style="margin-top: 8px;">
            <table class="trading-table">
              <thead>
                <tr><th>Token</th><th>Entry</th><th>Status</th><th>Opened</th><th>Closed</th></tr>
              </thead>
              <tbody>
                ${closedPositions
                  .map((p) => {
                    const statusLabel =
                      p.status === 'closed_sl'
                        ? '<span class="ms ms-sm">dangerous</span> Stop-Loss'
                        : p.status === 'closed_tp'
                          ? '<span class="ms ms-sm">flag</span> Take-Profit'
                          : '<span class="ms ms-sm">pan_tool</span> Manual';
                    return `<tr>
                    <td>${escHtml(p.symbol)}</td>
                    <td>$${formatPrice(p.entry_price_usd)}</td>
                    <td>${statusLabel}</td>
                    <td class="trading-time">${formatTime(p.created_at)}</td>
                    <td class="trading-time">${p.closed_at ? formatTime(p.closed_at) : '—'}</td>
                  </tr>`;
                  })
                  .join('')}
              </tbody>
            </table>
          </div>
        </details>
      </div>
    `
        : ''
    }
  </div>`;
}

function bindPositionEvents() {
  document.querySelectorAll('.pos-close-btn').forEach((btn) => {
    btn.addEventListener('click', async (e) => {
      const el = e.target as HTMLElement;
      const id = el.dataset.id;
      const symbol = el.dataset.symbol;
      if (!id) return;
      if (
        !(await confirmModal(
          `Close position for ${symbol}? This does NOT auto-sell — it just stops monitoring.`,
        ))
      )
        return;
      try {
        await pawEngine.positionClose(id);
        showTradingToast(`Position for ${symbol} closed`, 'success');
        loadTrading();
      } catch (err) {
        showTradingToast(`Failed to close: ${err}`, 'error');
      }
    });
  });
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
  _tradingToastTimer = window.setTimeout(
    () => {
      toast.style.display = 'none';
      _tradingToastTimer = null;
    },
    type === 'error' ? 8000 : 4000,
  );
}
