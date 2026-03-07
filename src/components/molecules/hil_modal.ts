// src/components/molecules/hil_modal.ts
// Human-In-the-Loop tool approval — inline chat card (VS Code style).
// Call initHILModal() once at app startup to register the Tauri event handler.
//
// VS Code-inspired approval UX:
//   1. Inline approval card injected into the chat stream (no modal popup)
//   2. "Always Allow" button — persist per-tool auto-approve
//   3. "Always Allow pattern" — auto-approve matching commands
//   4. Session override — approve all tools for N minutes
//   5. Collapsed tool details by default
//   6. Tier badge + risk banner (external / dangerous / unknown)
//   7. Type-to-confirm for critical-risk tools
//   8. Network audit info banner
//   9. OS notification when approval is pending

import { onEngineToolApproval, resolveEngineToolApproval } from '../../engine-bridge';
import type { EngineEvent } from '../../engine';
import {
  classifyCommandRisk,
  isPrivilegeEscalation,
  loadSecuritySettings,
  matchesAllowlist,
  matchesDenylist,
  auditNetworkRequest,
  getSessionOverrideRemaining,
  isFilesystemWriteTool,
  activateSessionOverride,
  extractCommandString,
  addToCommandAllowlist,
  type RiskClassification,
} from '../../security';
import { logCredentialActivity, logSecurityEvent } from '../../db';
import { showToast } from '../toast';
import { pushNotification } from '../notifications';
import { escHtml } from '../molecules/markdown';

// ── Persist "Always Allow" per tool in localStorage ─────────────────
const ALWAYS_ALLOW_KEY = 'paw-always-allow-tools';

function getAlwaysAllowedTools(): string[] {
  try {
    return JSON.parse(localStorage.getItem(ALWAYS_ALLOW_KEY) ?? '[]');
  } catch {
    return [];
  }
}

function addAlwaysAllowTool(toolName: string): void {
  const tools = getAlwaysAllowedTools();
  if (!tools.includes(toolName)) {
    tools.push(toolName);
    localStorage.setItem(ALWAYS_ALLOW_KEY, JSON.stringify(tools));
  }
}

/** Get all always-allowed tools (for use by bridge.ts) */
export function getAllAlwaysAllowedTools(): string[] {
  return getAlwaysAllowedTools();
}

// ── Generate a command pattern from tool name + args ────────────────
function generatePattern(toolName: string, args?: Record<string, unknown>): string | null {
  // For shell exec tools, extract the command prefix
  if ((toolName === 'exec' || toolName === 'run_command') && args) {
    const cmd = (args.command ?? args.cmd ?? '') as string;
    const firstWord = cmd.split(/\s+/)[0];
    if (firstWord) return `^${firstWord.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\b`;
  }
  // For fetch/web tools with URLs, extract the domain
  if ((toolName === 'fetch' || toolName === 'web_read') && args) {
    const url = (args.url ?? '') as string;
    try {
      const host = new URL(url).hostname;
      if (host) return host.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    } catch {
      /* not a URL */
    }
  }
  return null;
}

// ── Inline chat approval bubble ─────────────────────────────────────
// VS Code pattern: ChatConfirmationWidget with title → content → button bar.
// Resolved state collapses to a single-line summary (no faded full card).

/** Human-friendly labels for internal tool names */
const TOOL_LABELS: Record<string, string> = {
  exec: 'Run command',
  run_command: 'Run command',
  write_file: 'Write file',
  append_file: 'Append to file',
  delete_file: 'Delete file',
  email_send: 'Send email',
  webhook_send: 'Send webhook',
  rest_api_call: 'Call REST API',
  slack_send: 'Send Slack message',
  github_api: 'GitHub API call',
  sol_swap: 'Swap tokens (Solana)',
  sol_transfer: 'Transfer SOL',
  dex_swap: 'DEX swap',
  dex_transfer: 'DEX transfer',
  coinbase_trade: 'Coinbase trade',
  coinbase_transfer: 'Coinbase transfer',
};

/** Material icon for the tool tier */
function tierIcon(tier: string): string {
  switch (tier) {
    case 'dangerous':
      return 'warning';
    case 'external':
      return 'send';
    default:
      return 'gavel';
  }
}

interface BubbleOptions {
  toolCallId: string;
  toolName: string;
  args: Record<string, unknown> | undefined;
  tier: string;
  risk: RiskClassification | null;
  pattern: string | null;
  netAudit: { isNetworkRequest: boolean; targets: string[]; isExfiltration: boolean; exfiltrationReason?: string | null; allTargetsLocal: boolean };
  requireTypeToConfirm: boolean;
  onAllow: () => void;
  onDeny: () => void;
  onAlwaysAllow: () => void;
  onAlwaysPattern: () => void;
  onSessionOverride: (mins: number) => void;
}

function injectChatBubble(opts: BubbleOptions): HTMLElement | null {
  const chatMessages = document.getElementById('chat-messages');
  if (!chatMessages) return null;

  const { toolCallId, toolName, args, tier, risk, pattern, netAudit, requireTypeToConfirm } = opts;

  const bubble = document.createElement('div');
  bubble.className = `chat-approval-bubble${tier === 'external' ? ' bubble-external' : tier === 'dangerous' ? ' bubble-dangerous' : ''}`;
  bubble.dataset.toolCallId = toolCallId;

  const humanLabel = TOOL_LABELS[toolName] ?? toolName;
  const argsJson = args ? JSON.stringify(args, null, 2) : '';
  const isDangerous = risk && (risk.level === 'critical' || risk.level === 'high');
  const isCritical = risk?.level === 'critical';

  // Risk pill (only for elevated risk)
  const riskPill = risk
    ? `<span class="approval-risk-pill risk-${escHtml(risk.level)}">${escHtml(risk.level)}</span>`
    : '';

  // Risk banner (dangerous/critical inline — replaces modal risk banner)
  let riskBannerHtml = '';
  if (risk && isDangerous) {
    const riskIconChar = isCritical ? 'dangerous' : 'warning';
    riskBannerHtml = `
    <div class="chat-approval-risk-banner risk-${escHtml(risk.level)}">
      <span class="ms" style="font-size:14px">${riskIconChar}</span>
      <div>
        <strong>${escHtml(risk.level.toUpperCase())}: ${escHtml(risk.label)}</strong>
        <div class="risk-reason-text">${escHtml(risk.reason)}</div>
      </div>
    </div>`;
  }

  // Network audit banner
  let netBannerHtml = '';
  if (netAudit.isNetworkRequest) {
    const targetStr = netAudit.targets.length > 0 ? netAudit.targets.join(', ') : 'unknown destination';
    if (netAudit.isExfiltration) {
      netBannerHtml = `<div class="chat-approval-net-banner net-exfiltration"><span class="ms" style="font-size:14px">shield</span> <strong>Possible Data Exfiltration</strong> → ${escHtml(targetStr)}</div>`;
    } else if (!netAudit.allTargetsLocal) {
      netBannerHtml = `<div class="chat-approval-net-banner net-external"><span class="ms" style="font-size:14px">language</span> External request → ${escHtml(targetStr)}</div>`;
    } else {
      netBannerHtml = `<div class="chat-approval-net-banner net-local"><span class="ms" style="font-size:14px">dns</span> Localhost request → ${escHtml(targetStr)}</div>`;
    }
  }

  // Type-to-confirm (critical tools only)
  const typeConfirmHtml = (isCritical && requireTypeToConfirm) ? `
    <div class="chat-approval-type-confirm">
      <label>Type <strong>ALLOW</strong> to continue:</label>
      <input type="text" class="bubble-type-input" spellcheck="false" autocomplete="off" />
    </div>` : '';

  // Always-allow-pattern dropdown item
  const patternItemHtml = pattern ? `
          <button class="approval-dropdown-item bubble-pattern-btn">
            <span class="ms" style="font-size:14px">rule</span> Always allow pattern: <code>${escHtml(pattern)}</code>
          </button>` : '';

  // Session override dropdown items (not for dangerous tools)
  const sessionItems = isDangerous ? '' : `
          <button class="approval-dropdown-item bubble-session-btn" data-minutes="480">
            <span class="ms" style="font-size:14px">check_circle</span> Allow all for this session
          </button>
          <div class="approval-dropdown-divider"></div>
          <div class="approval-dropdown-label">Auto-approve for…</div>
          <button class="approval-dropdown-item bubble-session-btn" data-minutes="15">
            <span class="ms" style="font-size:14px">timer</span> 15 minutes
          </button>
          <button class="approval-dropdown-item bubble-session-btn" data-minutes="30">
            <span class="ms" style="font-size:14px">timer</span> 30 minutes
          </button>
          <button class="approval-dropdown-item bubble-session-btn" data-minutes="60">
            <span class="ms" style="font-size:14px">timer</span> 60 minutes
          </button>`;

  // Build the card: title bar → risk/net banners → content → buttons (VS Code layout)
  bubble.innerHTML = `
    <div class="chat-approval-title">
      <span class="ms approval-tier-icon">${tierIcon(tier)}</span>
      <span class="approval-title-text">${escHtml(humanLabel)}</span>
      ${riskPill}
    </div>
    <div class="chat-approval-subtitle"><code>${escHtml(toolName)}</code>${args && 'command' in args ? ` — <span class="approval-cmd-preview">${escHtml(String(args.command).slice(0, 120))}</span>` : ''}</div>
    ${riskBannerHtml}
    ${netBannerHtml}
    ${argsJson ? `
    <details class="chat-approval-details">
      <summary>Parameters</summary>
      <pre class="chat-approval-args-code"><code>${escHtml(argsJson)}</code></pre>
    </details>` : ''}
    ${typeConfirmHtml}
    <div class="chat-approval-buttons">
      <div class="approval-primary-group">
        <button class="btn btn-primary btn-sm bubble-allow-btn"${isCritical && requireTypeToConfirm ? ' disabled' : ''}>Continue</button>
        <button class="btn btn-primary btn-sm approval-dropdown-toggle bubble-more-btn" title="More options">
          <span class="ms">keyboard_arrow_down</span>
        </button>
        <div class="approval-dropdown-menu" style="display:none">
          ${sessionItems}
          ${sessionItems ? '<div class="approval-dropdown-divider"></div>' : ''}
          <button class="approval-dropdown-item bubble-always-btn">
            <span class="ms" style="font-size:14px">verified</span> Always allow <strong>${escHtml(toolName)}</strong>
          </button>
          ${patternItemHtml}
        </div>
      </div>
      <button class="btn btn-ghost btn-sm bubble-deny-btn">Skip</button>
    </div>
    <div class="chat-approval-resolved approved">
      <span class="ms" style="font-size:14px">check_circle</span> Approved
    </div>
    <div class="chat-approval-resolved denied">
      <span class="ms" style="font-size:14px">cancel</span> Denied
    </div>
  `;

  chatMessages.appendChild(bubble);
  chatMessages.scrollTop = chatMessages.scrollHeight;

  // Wire actions
  const allowBtn = bubble.querySelector('.bubble-allow-btn') as HTMLButtonElement | null;
  const denyBtn = bubble.querySelector('.bubble-deny-btn');
  const alwaysBtn = bubble.querySelector('.bubble-always-btn');
  const patternBtn = bubble.querySelector('.bubble-pattern-btn');
  const moreBtn = bubble.querySelector('.bubble-more-btn');
  const dropdownMenu = bubble.querySelector('.approval-dropdown-menu') as HTMLElement | null;
  const typeInput = bubble.querySelector('.bubble-type-input') as HTMLInputElement | null;
  const sessionBtns = bubble.querySelectorAll('.bubble-session-btn');

  const resolve = (approved: boolean) => {
    bubble.classList.add('resolved');
    const approvedEl = bubble.querySelector('.chat-approval-resolved.approved') as HTMLElement;
    const deniedEl = bubble.querySelector('.chat-approval-resolved.denied') as HTMLElement;
    if (approved) {
      if (approvedEl) approvedEl.style.display = 'flex';
    } else {
      if (deniedEl) deniedEl.style.display = 'flex';
    }
  };

  const closeDropdown = () => {
    if (dropdownMenu) dropdownMenu.style.display = 'none';
  };

  // Dropdown toggle
  moreBtn?.addEventListener('click', (e) => {
    e.stopPropagation();
    if (dropdownMenu) {
      dropdownMenu.style.display = dropdownMenu.style.display === 'none' ? 'flex' : 'none';
    }
  });

  // Close dropdown on outside click
  document.addEventListener('click', closeDropdown, { once: true });

  // Type-to-confirm wiring (critical tools)
  if (typeInput && allowBtn && isCritical && requireTypeToConfirm) {
    typeInput.addEventListener('input', () => {
      const val = typeInput.value.trim().toUpperCase();
      allowBtn.disabled = val !== 'ALLOW';
      allowBtn.textContent = val === 'ALLOW' ? 'Continue' : 'Type ALLOW';
    });
    // Focus the input for immediate typing
    requestAnimationFrame(() => typeInput.focus());
  }

  allowBtn?.addEventListener('click', () => {
    resolve(true);
    opts.onAllow();
  });
  denyBtn?.addEventListener('click', () => {
    resolve(false);
    opts.onDeny();
  });
  alwaysBtn?.addEventListener('click', () => {
    closeDropdown();
    resolve(true);
    opts.onAlwaysAllow();
  });
  patternBtn?.addEventListener('click', () => {
    closeDropdown();
    resolve(true);
    opts.onAlwaysPattern();
  });
  sessionBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      const mins = parseInt((btn as HTMLElement).dataset.minutes ?? '30', 10);
      closeDropdown();
      resolve(true);
      opts.onSessionOverride(mins);
    });
  });

  return bubble;
}

// ── Send OS notification (Notification API / Tauri notification) ─────
async function sendOSNotification(toolName: string): Promise<void> {
  // Try the web Notification API (works in Tauri webview with permission)
  if ('Notification' in window) {
    if (Notification.permission === 'granted') {
      new Notification('Open Pawz — Tool Approval Needed', {
        body: `The agent wants to use: ${toolName}`,
        icon: '/icons/128x128.png',
      });
    } else if (Notification.permission !== 'denied') {
      const permission = await Notification.requestPermission();
      if (permission === 'granted') {
        new Notification('Open Pawz — Tool Approval Needed', {
          body: `The agent wants to use: ${toolName}`,
          icon: '/icons/128x128.png',
        });
      }
    }
  }
}

export function initHILModal(): void {
  onEngineToolApproval((event: EngineEvent) => {
    const tc = event.tool_call;
    if (!tc) return;

    const toolCallId = tc.id;
    const toolName = tc.function?.name ?? 'unknown';
    const toolTier = event.tool_tier ?? 'unknown';
    let args: Record<string, unknown> | undefined;
    try {
      args = JSON.parse(tc.function?.arguments ?? '{}');
    } catch {
      args = undefined;
    }
    const sessionKey = event.session_id ?? '';

    const secSettings = loadSecuritySettings();
    const risk: RiskClassification | null = classifyCommandRisk(toolName, args);
    const cmdStr = extractCommandString(toolName, args);

    // ── "Always Allow" check: auto-approve if user previously set it ──
    const alwaysAllowed = getAlwaysAllowedTools();
    if (alwaysAllowed.includes(toolName) && toolTier !== 'dangerous') {
      resolveEngineToolApproval(toolCallId, true);
      logCredentialActivity({
        action: 'approved',
        toolName,
        detail: `[Engine] Always-allow: ${toolName}`,
        sessionKey,
        wasAllowed: true,
      });
      return;
    }

    // Network request audit
    const netAudit = auditNetworkRequest(toolName, args);
    if (netAudit.isNetworkRequest) {
      const targetStr =
        netAudit.targets.length > 0 ? netAudit.targets.join(', ') : '(unknown destination)';
      logSecurityEvent({
        eventType: 'network_request',
        riskLevel: netAudit.isExfiltration
          ? 'critical'
          : netAudit.allTargetsLocal
            ? null
            : 'medium',
        toolName,
        command: cmdStr,
        detail: `[Engine] Outbound request → ${targetStr}${netAudit.isExfiltration ? ' [EXFILTRATION SUSPECTED]' : ''}`,
        sessionKey,
        wasAllowed: true,
        matchedPattern: netAudit.isExfiltration
          ? `exfiltration:${netAudit.exfiltrationReason}`
          : 'network_tool',
      });
    }

    // Session override: auto-approve
    const overrideRemaining = getSessionOverrideRemaining();
    if (overrideRemaining > 0) {
      if (!(secSettings.autoDenyPrivilegeEscalation && isPrivilegeEscalation(toolName, args))) {
        resolveEngineToolApproval(toolCallId, true);
        const minsLeft = Math.ceil(overrideRemaining / 60000);
        logCredentialActivity({
          action: 'approved',
          toolName,
          detail: `[Engine] Session override (${minsLeft}min): ${toolName}`,
          sessionKey,
          wasAllowed: true,
        });
        return;
      }
    }

    // Read-only project mode
    if (secSettings.readOnlyProjects) {
      const writeCheck = isFilesystemWriteTool(toolName, args);
      if (writeCheck.isWrite) {
        resolveEngineToolApproval(toolCallId, false);
        logCredentialActivity({
          action: 'blocked',
          toolName,
          detail: `[Engine] Read-only mode: filesystem write blocked`,
          sessionKey,
          wasAllowed: false,
        });
        showToast('Blocked: filesystem writes are disabled (read-only project mode)', 'warning');
        return;
      }
    }

    // Auto-deny: privilege escalation
    if (secSettings.autoDenyPrivilegeEscalation && isPrivilegeEscalation(toolName, args)) {
      resolveEngineToolApproval(toolCallId, false);
      logCredentialActivity({
        action: 'blocked',
        toolName,
        detail: `[Engine] Auto-denied: privilege escalation`,
        sessionKey,
        wasAllowed: false,
      });
      showToast('Auto-denied: privilege escalation command blocked by security policy', 'warning');
      return;
    }

    // Auto-deny: critical risk
    if (secSettings.autoDenyCritical && risk?.level === 'critical') {
      resolveEngineToolApproval(toolCallId, false);
      logCredentialActivity({
        action: 'blocked',
        toolName,
        detail: `[Engine] Auto-denied: critical risk — ${risk.label}`,
        sessionKey,
        wasAllowed: false,
      });
      showToast(`Auto-denied: ${risk.label} — ${risk.reason}`, 'warning');
      return;
    }

    // Auto-deny: denylist
    if (
      secSettings.commandDenylist.length > 0 &&
      matchesDenylist(cmdStr, secSettings.commandDenylist)
    ) {
      resolveEngineToolApproval(toolCallId, false);
      logCredentialActivity({
        action: 'blocked',
        toolName,
        detail: `[Engine] Auto-denied: matched denylist`,
        sessionKey,
        wasAllowed: false,
      });
      showToast('Auto-denied: command matched your denylist', 'warning');
      return;
    }

    // Auto-approve: allowlist (only if no risk)
    if (
      !risk &&
      secSettings.commandAllowlist.length > 0 &&
      matchesAllowlist(cmdStr, secSettings.commandAllowlist)
    ) {
      resolveEngineToolApproval(toolCallId, true);
      logCredentialActivity({
        action: 'approved',
        toolName,
        detail: `[Engine] Auto-approved: allowlist match`,
        sessionKey,
        wasAllowed: true,
      });
      return;
    }

    // ── OS Notification (fires before showing UI) ───────────────────
    sendOSNotification(toolName);

    // ── Shared approval/deny handlers ───────────────────────────────
    const pattern = generatePattern(toolName, args);

    const doAllow = () => {
      resolveEngineToolApproval(toolCallId, true);
      const riskNote = risk ? ` (${risk.level}: ${risk.label})` : '';
      logCredentialActivity({
        action: 'approved',
        toolName,
        detail: `[Engine] User approved${riskNote}: ${toolName}`,
        sessionKey,
        wasAllowed: true,
      });
      logSecurityEvent({
        eventType: 'exec_approval',
        riskLevel: risk?.level ?? null,
        toolName,
        command: cmdStr,
        detail: `[Engine] User approved${riskNote}`,
        sessionKey,
        wasAllowed: true,
        matchedPattern: risk?.matchedPattern,
      });
      showToast('Tool approved', 'success');
      pushNotification('hil', 'Tool approved', toolName, undefined, 'chat');
    };

    const doDeny = () => {
      resolveEngineToolApproval(toolCallId, false);
      const riskNote = risk ? ` (${risk.level}: ${risk.label})` : '';
      logCredentialActivity({
        action: 'denied',
        toolName,
        detail: `[Engine] User denied${riskNote}: ${toolName}`,
        sessionKey,
        wasAllowed: false,
      });
      logSecurityEvent({
        eventType: 'exec_approval',
        riskLevel: risk?.level ?? null,
        toolName,
        command: cmdStr,
        detail: `[Engine] User denied${riskNote}`,
        sessionKey,
        wasAllowed: false,
        matchedPattern: risk?.matchedPattern,
      });
      showToast('Tool denied', 'warning');
      pushNotification('hil', 'Tool denied', toolName, undefined, 'chat');
    };

    const doAlwaysAllow = () => {
      addAlwaysAllowTool(toolName);
      doAllow();
      showToast(`"${toolName}" will be auto-approved from now on`, 'success');
    };

    const doAlwaysPattern = () => {
      if (pattern) {
        addToCommandAllowlist(pattern);
        doAllow();
        showToast(`Commands matching "${pattern}" will be auto-approved`, 'success');
      }
    };

    // ── Inject inline chat approval card (no modal popup) ─────────────
    const chatBubble = injectChatBubble({
      toolCallId,
      toolName,
      args,
      tier: toolTier,
      risk,
      pattern,
      netAudit,
      requireTypeToConfirm: !!(secSettings.requireTypeToCritical && risk?.level === 'critical'),
      onAllow: doAllow,
      onDeny: doDeny,
      onAlwaysAllow: doAlwaysAllow,
      onAlwaysPattern: doAlwaysPattern,
      onSessionOverride: (mins: number) => {
        activateSessionOverride(mins);
        resolveEngineToolApproval(toolCallId, true);
        logCredentialActivity({
          action: 'approved',
          toolName,
          detail: `[Engine] Session override (${mins}min): ${toolName}`,
          sessionKey,
          wasAllowed: true,
        });
        showToast(
          `Session override active for ${mins} minutes — all tool requests auto-approved`,
          'info',
        );
      },
    });

    // Keep reference for external resolution (if needed)
    void chatBubble;

    // Notify: tool needs approval (important — user may be in another view)
    pushNotification('hil', 'Tool approval needed', toolName, undefined, 'chat');
  });
}
