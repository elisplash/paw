// src/components/molecules/hil_modal.ts
// Human-In-the-Loop tool approval modal + inline chat bubble.
// Call initHILModal() once at app startup to register the Tauri event handler.
//
// VS Code-inspired approval UX:
//   1. Inline approval bubble injected into the chat stream
//   2. Modal overlay as fallback (always shown for dangerous tools)
//   3. "Always Allow" button — persist per-tool auto-approve
//   4. "Always Allow pattern" — auto-approve matching commands
//   5. Collapsed tool details by default
//   6. Tier badge (external / dangerous / unknown)
//   7. OS notification when approval is pending

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

const $ = (id: string) => document.getElementById(id);

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

function injectChatBubble(
  toolCallId: string,
  toolName: string,
  args: Record<string, unknown> | undefined,
  tier: string,
  risk: RiskClassification | null,
  onAllow: () => void,
  onDeny: () => void,
  onAlwaysAllow: () => void,
): HTMLElement | null {
  const chatMessages = document.getElementById('chat-messages');
  if (!chatMessages) return null;

  const bubble = document.createElement('div');
  bubble.className = `chat-approval-bubble${tier === 'external' ? ' bubble-external' : tier === 'dangerous' ? ' bubble-dangerous' : ''}`;
  bubble.dataset.toolCallId = toolCallId;

  const humanLabel = TOOL_LABELS[toolName] ?? toolName;
  const argsJson = args ? JSON.stringify(args, null, 2) : '';

  // Risk pill (only for elevated risk)
  const riskPill = risk
    ? `<span class="approval-risk-pill risk-${escHtml(risk.level)}">${escHtml(risk.level)}</span>`
    : '';

  // Build the card: title bar → content → button bar (VS Code layout)
  bubble.innerHTML = `
    <div class="chat-approval-title">
      <span class="ms approval-tier-icon">${tierIcon(tier)}</span>
      <span class="approval-title-text">${escHtml(humanLabel)}</span>
      ${riskPill}
    </div>
    <div class="chat-approval-subtitle"><code>${escHtml(toolName)}</code>${args && 'command' in args ? ` — <span class="approval-cmd-preview">${escHtml(String(args.command).slice(0, 120))}</span>` : ''}</div>
    ${argsJson ? `
    <details class="chat-approval-details">
      <summary>Parameters</summary>
      <pre class="chat-approval-args-code"><code>${escHtml(argsJson)}</code></pre>
    </details>` : ''}
    <div class="chat-approval-buttons">
      <div class="approval-primary-group">
        <button class="btn btn-primary btn-sm bubble-allow-btn">Continue</button>
        <button class="btn btn-primary btn-sm approval-dropdown-toggle bubble-more-btn" title="More options">
          <span class="ms" style="font-size:12px">expand_more</span>
        </button>
        <div class="approval-dropdown-menu" style="display:none">
          <button class="approval-dropdown-item bubble-always-btn">
            <span class="ms" style="font-size:14px">verified</span> Always allow <strong>${escHtml(toolName)}</strong>
          </button>
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
  const allowBtn = bubble.querySelector('.bubble-allow-btn');
  const denyBtn = bubble.querySelector('.bubble-deny-btn');
  const alwaysBtn = bubble.querySelector('.bubble-always-btn');
  const moreBtn = bubble.querySelector('.bubble-more-btn');
  const dropdownMenu = bubble.querySelector('.approval-dropdown-menu') as HTMLElement | null;

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

  // Dropdown toggle
  moreBtn?.addEventListener('click', (e) => {
    e.stopPropagation();
    if (dropdownMenu) {
      dropdownMenu.style.display = dropdownMenu.style.display === 'none' ? 'flex' : 'none';
    }
  });

  // Close dropdown on outside click
  const closeDropdown = () => {
    if (dropdownMenu) dropdownMenu.style.display = 'none';
  };
  document.addEventListener('click', closeDropdown, { once: true });

  allowBtn?.addEventListener('click', () => {
    resolve(true);
    onAllow();
  });
  denyBtn?.addEventListener('click', () => {
    resolve(false);
    onDeny();
  });
  alwaysBtn?.addEventListener('click', () => {
    closeDropdown();
    resolve(true);
    onAlwaysAllow();
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
    const desc = `The agent wants to use: ${toolName}`;
    const sessionKey = event.session_id ?? '';

    const modal = $('approval-modal');
    const modalCard = $('approval-modal-card');
    const modalTitle = $('approval-modal-title');
    const descEl = $('approval-modal-desc');
    const detailsEl = $('approval-modal-details');
    const detailsToggle = $('approval-details-toggle') as HTMLDetailsElement | null;
    const riskBanner = $('approval-risk-banner');
    const riskIcon = $('approval-risk-icon');
    const riskLabel = $('approval-risk-label');
    const riskReason = $('approval-risk-reason');
    const typeConfirm = $('approval-type-confirm');
    const typeInput = $('approval-type-input') as HTMLInputElement | null;
    const allowBtn = $('approval-allow-btn') as HTMLButtonElement | null;
    const tierBadge = $('approval-tier-badge');
    const alwaysActions = $('approval-always-actions');
    const alwaysToolName = $('approval-always-tool-name');
    const alwaysPatternBtn = $('approval-always-pattern-btn');
    const alwaysPatternText = $('approval-always-pattern-text');
    if (!modal || !descEl) return;

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
    const isDangerous = risk && (risk.level === 'critical' || risk.level === 'high');
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

    // ── Inject inline chat bubble ───────────────────────────────────
    const chatBubble = injectChatBubble(
      toolCallId,
      toolName,
      args,
      toolTier,
      risk,
      () => {
        cleanupModal();
        doAllow();
      },
      () => {
        cleanupModal();
        doDeny();
      },
      () => {
        cleanupModal();
        doAlwaysAllow();
      },
    );

    // Notify: tool needs approval (important — user may be in another view)
    pushNotification('hil', 'Tool approval needed', toolName, undefined, 'chat');

    // ── Show modal (always for dangerous, alongside bubble for others) ──
    const isCritical = risk?.level === 'critical';

    modalCard?.classList.remove('danger-modal');
    riskBanner?.classList.remove('risk-critical', 'risk-high', 'risk-medium');
    if (riskBanner) riskBanner.style.display = 'none';
    if (typeConfirm) typeConfirm.style.display = 'none';
    if (typeInput) typeInput.value = '';
    if (allowBtn) {
      allowBtn.disabled = false;
      allowBtn.textContent = 'Allow';
    }
    if (modalTitle) modalTitle.textContent = 'Tool Approval Required';
    if (detailsToggle) detailsToggle.open = false;

    // Tier badge
    if (tierBadge) {
      tierBadge.className = 'approval-tier-badge';
      if (toolTier === 'external') {
        tierBadge.textContent = 'External';
        tierBadge.classList.add('tier-external');
      } else if (toolTier === 'dangerous') {
        tierBadge.textContent = 'Dangerous';
        tierBadge.classList.add('tier-dangerous');
      } else {
        tierBadge.textContent = toolTier;
        tierBadge.classList.add('tier-unknown');
      }
    }

    // Always Allow buttons
    if (alwaysActions) alwaysActions.style.display = isDangerous ? 'none' : 'flex';
    if (alwaysToolName) alwaysToolName.textContent = toolName;
    if (alwaysPatternBtn && alwaysPatternText) {
      if (pattern) {
        alwaysPatternBtn.style.display = '';
        alwaysPatternText.textContent = pattern;
      } else {
        alwaysPatternBtn.style.display = 'none';
      }
    }

    if (risk) {
      if (isDangerous) {
        modalCard?.classList.add('danger-modal');
        if (modalTitle) modalTitle.textContent = 'Dangerous Command Detected';
      }
      if (riskBanner && riskLabel && riskReason && riskIcon) {
        riskBanner.style.display = 'flex';
        riskBanner.classList.add(`risk-${risk.level}`);
        riskLabel.textContent = `${risk.level.toUpperCase()}: ${risk.label}`;
        riskReason.textContent = risk.reason;
        riskIcon.textContent = isCritical ? '☠' : risk.level === 'high' ? '!' : '⚠';
      }
      if (isCritical && secSettings.requireTypeToCritical && typeConfirm && typeInput && allowBtn) {
        typeConfirm.style.display = 'block';
        allowBtn.disabled = true;
        allowBtn.textContent = 'Type ALLOW first';
        const onTypeInput = () => {
          const val = typeInput.value.trim().toUpperCase();
          allowBtn.disabled = val !== 'ALLOW';
          allowBtn.textContent = val === 'ALLOW' ? 'Allow' : 'Type ALLOW first';
        };
        typeInput.addEventListener('input', onTypeInput);
        (typeInput as unknown as Record<string, unknown>)._secCleanup = onTypeInput;
      }
    }

    descEl.textContent = desc;

    // Network audit banner
    const netBanner = $('approval-network-banner');
    if (netBanner) netBanner.style.display = 'none';
    if (netAudit.isNetworkRequest && netBanner) {
      netBanner.style.display = 'block';
      const targetStr =
        netAudit.targets.length > 0 ? netAudit.targets.join(', ') : 'unknown destination';
      if (netAudit.isExfiltration) {
        netBanner.className = 'network-banner network-exfiltration';
        netBanner.innerHTML = `<strong>Possible Data Exfiltration</strong><br>Outbound data transfer detected → ${escHtml(targetStr)}`;
      } else if (!netAudit.allTargetsLocal) {
        netBanner.className = 'network-banner network-external';
        netBanner.innerHTML = `<strong>External Network Request</strong><br>Destination: ${escHtml(targetStr)}`;
      } else {
        netBanner.className = 'network-banner network-local';
        netBanner.innerHTML = `<strong>Localhost Request</strong><br>Destination: ${escHtml(targetStr)}`;
      }
    }

    if (detailsEl) {
      detailsEl.innerHTML = args
        ? `<pre class="code-block"><code>${escHtml(JSON.stringify(args, null, 2))}</code></pre>`
        : '';
    }
    modal.style.display = 'flex';

    const cleanupModal = () => {
      modal.style.display = 'none';
      if (typeInput) {
        const fn = (typeInput as unknown as Record<string, unknown>)._secCleanup as
          | (() => void)
          | undefined;
        if (fn) typeInput.removeEventListener('input', fn);
      }
      $('approval-allow-btn')?.removeEventListener('click', onModalAllow);
      $('approval-deny-btn')?.removeEventListener('click', onModalDeny);
      $('approval-modal-close')?.removeEventListener('click', onModalDeny);
      $('approval-always-allow-btn')?.removeEventListener('click', onModalAlwaysAllow);
      $('approval-always-pattern-btn')?.removeEventListener('click', onModalAlwaysPattern);
    };

    const resolveInlineBubble = (approved: boolean) => {
      if (chatBubble) {
        chatBubble.classList.add('resolved');
        const approvedEl = chatBubble.querySelector(
          '.chat-approval-resolved.approved',
        ) as HTMLElement;
        const deniedEl = chatBubble.querySelector(
          '.chat-approval-resolved.denied',
        ) as HTMLElement;
        if (approved && approvedEl) approvedEl.style.display = 'flex';
        if (!approved && deniedEl) deniedEl.style.display = 'flex';
      }
    };

    const onModalAllow = () => {
      cleanupModal();
      resolveInlineBubble(true);
      doAllow();
    };
    const onModalDeny = () => {
      cleanupModal();
      resolveInlineBubble(false);
      doDeny();
    };
    const onModalAlwaysAllow = () => {
      cleanupModal();
      resolveInlineBubble(true);
      doAlwaysAllow();
    };
    const onModalAlwaysPattern = () => {
      cleanupModal();
      resolveInlineBubble(true);
      doAlwaysPattern();
    };

    $('approval-allow-btn')?.addEventListener('click', onModalAllow);
    $('approval-deny-btn')?.addEventListener('click', onModalDeny);
    $('approval-modal-close')?.addEventListener('click', onModalDeny);
    $('approval-always-allow-btn')?.addEventListener('click', onModalAlwaysAllow);
    $('approval-always-pattern-btn')?.addEventListener('click', onModalAlwaysPattern);

    // Session override dropdown
    const overrideBtn = $('session-override-btn');
    const overrideMenu = $('session-override-menu');
    if (overrideBtn && overrideMenu) {
      const toggleMenu = (e: Event) => {
        e.stopPropagation();
        overrideMenu.style.display = overrideMenu.style.display === 'none' ? 'flex' : 'none';
      };
      overrideBtn.addEventListener('click', toggleMenu);
      overrideMenu.querySelectorAll('.session-override-opt').forEach((opt) => {
        opt.addEventListener('click', () => {
          const mins = parseInt((opt as HTMLElement).dataset.minutes ?? '30', 10);
          activateSessionOverride(mins);
          overrideMenu.style.display = 'none';
          cleanupModal();
          resolveInlineBubble(true);
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
        });
      });
    }
  });
}
