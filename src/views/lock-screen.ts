// src/views/lock-screen.ts — Lock screen with system auth / passphrase gate
// On unlock, shows ALL available methods (system password AND passphrase) so
// the user can choose whichever they prefer — no need to sign in twice.
// Lock mode in localStorage: "system" | "passphrase" | "both" | "none"
//
// All event listeners are bound ONCE. Form switching is done via display toggling.

import { invoke } from '@tauri-apps/api/core';

const $ = (id: string) => document.getElementById(id);
const LOCK_MODE_KEY = 'paw-lock-mode';

type LockMode = 'system' | 'passphrase' | 'both' | 'none';

function getLockMode(): LockMode | null {
  return localStorage.getItem(LOCK_MODE_KEY) as LockMode | null;
}

function setLockMode(mode: LockMode) {
  localStorage.setItem(LOCK_MODE_KEY, mode);
}

// ── Shared state ─────────────────────────────────────────────────────────

let _lockScreen: HTMLElement;
let _resolve: () => void;
let _unlocked = false;
// Track what is actually available for the combined unlock view
let _setupBothMode = false; // true when setup flow picked "Both"

/**
 * Initialize the lock screen. Resolves when the user is authenticated.
 */
export function initLockScreen(): Promise<void> {
  return new Promise(async (resolve) => {
    const lockScreen = $('lock-screen');
    if (!lockScreen) { resolve(); return; }

    const hasTauri = !!(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    if (!hasTauri) { hideLockScreen(lockScreen); resolve(); return; }

    _lockScreen = lockScreen;
    _resolve = resolve;

    wireListeners();

    const mode = getLockMode();

    if (!mode) {
      await showSetupForm();
    } else if (mode === 'none') {
      hideLockScreen(lockScreen);
      resolve();
    } else {
      // For any configured mode, show the combined unlock screen
      // which displays all available methods
      await showUnlockScreen();
    }
  });
}

// ── Wire all listeners once ──────────────────────────────────────────────

function wireListeners() {
  // System auth button
  $('lock-system-btn')!.addEventListener('click', triggerSystemAuth);

  // Passphrase unlock
  $('lock-submit')!.addEventListener('click', tryUnlock);
  $('lock-password')!.addEventListener('keydown', (e) => {
    if ((e as KeyboardEvent).key === 'Enter') { e.preventDefault(); tryUnlock(); }
  });

  // Reset link
  $('lock-reset')!.addEventListener('click', handleReset);

  // Setup form — option cards
  $('lock-opt-system')!.addEventListener('click', handleSetupSystem);
  $('lock-opt-passphrase')!.addEventListener('click', handleSetupPassphrase);
  $('lock-opt-both')!.addEventListener('click', handleSetupBoth);
  $('lock-opt-skip')!.addEventListener('click', handleSetupSkip);
  $('lock-setup-back')!.addEventListener('click', handleSetupBack);
  $('lock-setup-submit')!.addEventListener('click', trySetup);
  $('lock-new-password')!.addEventListener('keydown', (e) => {
    if ((e as KeyboardEvent).key === 'Enter') { e.preventDefault(); ($('lock-confirm-password') as HTMLInputElement).focus(); }
  });
  $('lock-confirm-password')!.addEventListener('keydown', (e) => {
    if ((e as KeyboardEvent).key === 'Enter') { e.preventDefault(); trySetup(); }
  });
}

// ── Combined Unlock Screen ───────────────────────────────────────────────
// Shows all available methods at once: system auth button + passphrase input

async function showUnlockScreen() {
  hideAllForms();

  const sysAvail = await invoke<boolean>('lock_screen_system_available').catch(() => false);
  const hasPass = await invoke<boolean>('lock_screen_has_passphrase').catch(() => false);

  // If neither is available, the keychain was cleared — reset to setup
  if (!sysAvail && !hasPass) {
    localStorage.removeItem(LOCK_MODE_KEY);
    await showSetupForm();
    return;
  }

  $('lock-subtitle')!.textContent = 'Verify your identity';

  // Show system auth button if available
  if (sysAvail) {
    $('lock-form-system')!.style.display = '';
    $('lock-system-error')!.textContent = '';
  }

  // Show divider if BOTH are available
  if (sysAvail && hasPass) {
    $('lock-divider')!.style.display = '';
  }

  // Show passphrase input if a passphrase exists
  if (hasPass) {
    $('lock-form-unlock')!.style.display = '';
    $('lock-error')!.textContent = '';
    ($('lock-password') as HTMLInputElement).value = '';
    if (!sysAvail) {
      // Only passphrase available — focus it
      requestAnimationFrame(() => ($('lock-password') as HTMLInputElement).focus());
    }
  }

  // Show footer
  $('lock-footer-links')!.style.display = '';
}

// ── System Authentication ────────────────────────────────────────────────

async function triggerSystemAuth() {
  if (_unlocked) return;
  const btn = $('lock-system-btn') as HTMLButtonElement;
  const errorEl = $('lock-system-error')!;
  btn.disabled = true;
  errorEl.textContent = '';
  try {
    const success = await invoke<boolean>('lock_screen_system_auth');
    if (success) {
      unlock();
    } else {
      errorEl.textContent = 'Authentication cancelled or failed';
      btn.disabled = false;
    }
  } catch (e) {
    errorEl.textContent = `${e instanceof Error ? e.message : String(e)}`;
    btn.disabled = false;
  }
}

// ── Passphrase Unlock ────────────────────────────────────────────────────

async function tryUnlock() {
  if (_unlocked) return;
  const passwordInput = $('lock-password') as HTMLInputElement;
  const submitBtn = $('lock-submit') as HTMLButtonElement;
  const errorEl = $('lock-error')!;
  const passphrase = passwordInput.value;

  if (!passphrase) { errorEl.textContent = 'Please enter your passphrase'; shakeInput(passwordInput); return; }

  submitBtn.disabled = true;
  errorEl.textContent = '';
  try {
    const valid = await invoke<boolean>('lock_screen_verify_passphrase', { passphrase });
    if (valid) {
      unlock();
    } else {
      errorEl.textContent = 'Incorrect passphrase';
      shakeInput(passwordInput);
      passwordInput.value = '';
      passwordInput.focus();
    }
  } catch (e) {
    errorEl.textContent = `Error: ${e instanceof Error ? e.message : String(e)}`;
  } finally {
    submitBtn.disabled = false;
  }
}

// ── First-Time Setup ─────────────────────────────────────────────────────

async function showSetupForm() {
  hideAllForms();
  $('lock-form-setup')!.style.display = '';
  $('lock-auth-options')!.style.display = '';
  $('lock-passphrase-subform')!.style.display = 'none';
  $('lock-subtitle')!.textContent = 'Welcome to OpenPawz';
  $('lock-setup-error')!.textContent = '';
  _setupBothMode = false;

  // Hide system-related options if not available
  const avail = await invoke<boolean>('lock_screen_system_available').catch(() => false);
  $('lock-opt-system')!.style.display = avail ? '' : 'none';
  $('lock-opt-both')!.style.display = avail ? '' : 'none';
}

async function handleSetupSystem() {
  if (_unlocked) return;
  try {
    const success = await invoke<boolean>('lock_screen_system_auth');
    if (success) {
      setLockMode('system');
      unlock();
    }
  } catch (e) {
    $('lock-subtitle')!.textContent = `${e instanceof Error ? e.message : String(e)}`;
  }
}

function handleSetupPassphrase() {
  _setupBothMode = false;
  $('lock-auth-options')!.style.display = 'none';
  $('lock-passphrase-subform')!.style.display = '';
  $('lock-subtitle')!.textContent = 'Create a passphrase';
  ($('lock-new-password') as HTMLInputElement).value = '';
  ($('lock-confirm-password') as HTMLInputElement).value = '';
  $('lock-setup-error')!.textContent = '';
  requestAnimationFrame(() => ($('lock-new-password') as HTMLInputElement).focus());
}

function handleSetupBoth() {
  _setupBothMode = true;
  $('lock-auth-options')!.style.display = 'none';
  $('lock-passphrase-subform')!.style.display = '';
  $('lock-subtitle')!.textContent = 'Create a passphrase (system auth will also be enabled)';
  ($('lock-new-password') as HTMLInputElement).value = '';
  ($('lock-confirm-password') as HTMLInputElement).value = '';
  $('lock-setup-error')!.textContent = '';
  requestAnimationFrame(() => ($('lock-new-password') as HTMLInputElement).focus());
}

function handleSetupSkip() {
  if (_unlocked) return;
  setLockMode('none');
  unlock();
}

function handleSetupBack() {
  _setupBothMode = false;
  $('lock-auth-options')!.style.display = '';
  $('lock-passphrase-subform')!.style.display = 'none';
  $('lock-subtitle')!.textContent = 'Welcome to OpenPawz';
  $('lock-setup-error')!.textContent = '';
}

async function trySetup() {
  if (_unlocked) return;
  const newPasswordInput = $('lock-new-password') as HTMLInputElement;
  const confirmPasswordInput = $('lock-confirm-password') as HTMLInputElement;
  const submitBtn = $('lock-setup-submit') as HTMLButtonElement;
  const errorEl = $('lock-setup-error')!;
  const newPass = newPasswordInput.value;
  const confirmPass = confirmPasswordInput.value;

  if (!newPass) { errorEl.textContent = 'Please enter a passphrase'; shakeInput(newPasswordInput); return; }
  if (newPass.length < 4) { errorEl.textContent = 'Passphrase must be at least 4 characters'; shakeInput(newPasswordInput); return; }
  if (newPass !== confirmPass) {
    errorEl.textContent = 'Passphrases do not match';
    shakeInput(confirmPasswordInput);
    confirmPasswordInput.value = '';
    confirmPasswordInput.focus();
    return;
  }

  submitBtn.disabled = true;
  errorEl.textContent = '';
  try {
    await invoke('lock_screen_set_passphrase', { passphrase: newPass });
    setLockMode(_setupBothMode ? 'both' : 'passphrase');
    unlock();
  } catch (e) {
    errorEl.textContent = `Error: ${e instanceof Error ? e.message : String(e)}`;
  } finally {
    submitBtn.disabled = false;
  }
}

// ── Reset ────────────────────────────────────────────────────────────────

async function handleReset() {
  localStorage.removeItem(LOCK_MODE_KEY);
  try { await invoke('lock_screen_remove_passphrase'); } catch { /* ignore */ }
  await showSetupForm();
}

// ── Helpers ──────────────────────────────────────────────────────────────

function hideAllForms() {
  for (const id of ['lock-form-system', 'lock-form-unlock', 'lock-form-setup', 'lock-divider', 'lock-footer-links']) {
    const el = $(id);
    if (el) el.style.display = 'none';
  }
}

function unlock() {
  if (_unlocked) return;
  _unlocked = true;
  _lockScreen.classList.add('lock-unlocking');
  setTimeout(() => {
    _lockScreen.classList.add('lock-unlocked');
    setTimeout(() => {
      _lockScreen.classList.add('lock-hidden');
      _resolve();
    }, 400);
  }, 300);
}

function hideLockScreen(lockScreen: HTMLElement) {
  lockScreen.classList.add('lock-hidden');
}

function shakeInput(el: HTMLElement) {
  el.classList.remove('lock-shake');
  void el.offsetWidth;
  el.classList.add('lock-shake');
}
