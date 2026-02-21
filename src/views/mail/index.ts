// Mail View — Index (orchestration, state, event wiring, exports)

import { $ } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { isConnected } from '../../state/connection';
import {
  configureMolecules,
  renderMailAccounts,
  loadMailInbox,
  renderMailList,
  showMailEmpty,
  getMailAccountsRef,
  setMailFolder,
  setMailSelectedId,
  setOpenMailAccountSetup,
  setLoadMailRef,
} from './molecules';
import {
  setCloseChannelSetupFn,
  openMailAccountSetup as _openMailAccountSetup,
  setLoadMailRefSetup,
} from './setup';

// ── Wire injected dependencies (break circular imports) ────────────────────

setOpenMailAccountSetup(() => _openMailAccountSetup());
setLoadMailRef(() => loadMail());
setLoadMailRefSetup(() => loadMail());

// ── Public re-exports ──────────────────────────────────────────────────────

export type { MailPermissions, MailAccount, MailMessage } from './atoms';
export { loadMailPermissions, extractContent } from './atoms';
export {
  openMailAccountSetup,
  saveMailImapSetup,
  getChannelSetupType,
  clearChannelSetupType,
} from './setup';
export {
  renderMailAccounts,
  renderMailList,
  showMailEmpty,
  openMailMessage,
  openComposeModal,
} from './molecules';

// ── Configure ──────────────────────────────────────────────────────────────

export function configure(opts: {
  switchView: (view: string) => void;
  setCurrentSession: (key: string | null) => void;
  getChatInput: () => HTMLTextAreaElement | null;
  closeChannelSetup: () => void;
}) {
  configureMolecules({
    switchView: opts.switchView,
    setCurrentSession: opts.setCurrentSession,
    getChatInput: opts.getChatInput,
  });
  setCloseChannelSetupFn(opts.closeChannelSetup);
}

// ── State accessors ────────────────────────────────────────────────────────

export function getMailAccounts(): { name: string; email: string }[] {
  return getMailAccountsRef();
}

// ── Main loader ────────────────────────────────────────────────────────────

export async function loadMail(): Promise<void> {
  if (!isConnected()) {
    console.warn('[mail] loadMail skipped — engine not connected');
    // Still try to render local accounts so user sees their config
    await renderMailAccounts(null, null);
    if (getMailAccountsRef().length === 0) {
      showMailEmpty(true);
    }
    return;
  }
  try {
    await renderMailAccounts(null, null);

    const hasAccounts = getMailAccountsRef().length > 0;
    if (hasAccounts) {
      await loadMailInbox();
    } else {
      renderMailList();
      showMailEmpty(true);
    }
  } catch (e) {
    console.warn('[mail] Load failed:', e);
    showMailEmpty(true);
  }
}

// ── Event wiring ───────────────────────────────────────────────────────────

export function initMailEvents(): void {
  // Compose new email
  $('mail-compose')?.addEventListener('click', () => {
    showToast('Himalaya skill is required to send emails. Enable it in the Skills view.', 'error');
  });

  // Mail folder switching
  document.querySelectorAll('.mail-folder').forEach((folder) => {
    folder.addEventListener('click', () => {
      document.querySelectorAll('.mail-folder').forEach((f) => f.classList.remove('active'));
      folder.classList.add('active');
      const folderName = folder.getAttribute('data-folder') ?? 'inbox';
      setMailFolder(folderName);
      const titleEl = $('mail-folder-title');
      if (titleEl) {
        const labels: Record<string, string> = {
          inbox: 'Inbox',
          drafts: 'Drafts',
          sent: 'Sent',
          agent: 'Agent Drafts',
        };
        titleEl.textContent = labels[folderName] ?? folderName;
      }
      renderMailList();
      const preview = $('mail-preview');
      if (preview)
        preview.innerHTML = '<div class="mail-preview-empty">Select an email to read</div>';
      setMailSelectedId(null);
    });
  });

  // Refresh
  $('mail-refresh')?.addEventListener('click', () => loadMail());

  // Add account buttons
  $('mail-add-account')?.addEventListener('click', () => _openMailAccountSetup());
}
