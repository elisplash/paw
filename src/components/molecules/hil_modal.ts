// src/components/molecules/hil_modal.ts
// Human-In-the-Loop tool approval modal.
// Call initHILModal() once at app startup to register the Tauri event handler.

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
  type RiskClassification,
} from '../../security';
import { logCredentialActivity, logSecurityEvent } from '../../db';
import { showToast } from '../toast';
import { escHtml } from '../molecules/markdown';

const $ = (id: string) => document.getElementById(id);

export function initHILModal(): void {
  onEngineToolApproval((event: EngineEvent) => {
    const tc = event.tool_call;
    if (!tc) return;

    const toolCallId = tc.id;
    const toolName = tc.function?.name ?? 'unknown';
    let args: Record<string, unknown> | undefined;
    try {
      args = JSON.parse(tc.function?.arguments ?? '{}');
    } catch {
      args = undefined;
    }
    const desc = `The agent wants to use tool: ${toolName}`;
    const sessionKey = event.session_id ?? '';

    const modal = $('approval-modal');
    const modalCard = $('approval-modal-card');
    const modalTitle = $('approval-modal-title');
    const descEl = $('approval-modal-desc');
    const detailsEl = $('approval-modal-details');
    const riskBanner = $('approval-risk-banner');
    const riskIcon = $('approval-risk-icon');
    const riskLabel = $('approval-risk-label');
    const riskReason = $('approval-risk-reason');
    const typeConfirm = $('approval-type-confirm');
    const typeInput = $('approval-type-input') as HTMLInputElement | null;
    const allowBtn = $('approval-allow-btn') as HTMLButtonElement | null;
    if (!modal || !descEl) return;

    const secSettings = loadSecuritySettings();
    const risk: RiskClassification | null = classifyCommandRisk(toolName, args);
    const cmdStr = extractCommandString(toolName, args);

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

    // ── Show modal ──
    const isDangerous = risk && (risk.level === 'critical' || risk.level === 'high');
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

    const cleanup = () => {
      modal.style.display = 'none';
      if (typeInput) {
        const fn = (typeInput as unknown as Record<string, unknown>)._secCleanup as
          | (() => void)
          | undefined;
        if (fn) typeInput.removeEventListener('input', fn);
      }
      $('approval-allow-btn')?.removeEventListener('click', onAllow);
      $('approval-deny-btn')?.removeEventListener('click', onDeny);
      $('approval-modal-close')?.removeEventListener('click', onDeny);
    };

    const onAllow = () => {
      cleanup();
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
    };
    const onDeny = () => {
      cleanup();
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
    };

    $('approval-allow-btn')?.addEventListener('click', onAllow);
    $('approval-deny-btn')?.addEventListener('click', onDeny);
    $('approval-modal-close')?.addEventListener('click', onDeny);

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
          cleanup();
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
