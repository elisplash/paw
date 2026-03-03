// Toast notification component — animated with anime.js

import { toastEnter, toastLeave } from './animations';

const $ = (id: string) => document.getElementById(id);

let _toastTimer: ReturnType<typeof setTimeout> | null = null;

export function showToast(
  message: string,
  type: 'info' | 'success' | 'error' | 'warning' = 'info',
  durationMs = 3500,
) {
  const toast = $('global-toast');
  if (!toast) return;

  // Clear any pending dismiss timer
  if (_toastTimer) {
    clearTimeout(_toastTimer);
    _toastTimer = null;
  }

  toast.textContent = message;
  toast.className = `global-toast toast-${type}`;
  toast.style.display = 'block';
  toast.style.opacity = '0';

  // Animate in with spring physics
  toastEnter(toast);

  // Schedule animated dismiss
  _toastTimer = setTimeout(() => {
    const anim = toastLeave(toast);
    anim.then(() => {
      toast.style.display = 'none';
    });
  }, durationMs);
}
