// Toast notification component

const $ = (id: string) => document.getElementById(id);

export function showToast(
  message: string,
  type: 'info' | 'success' | 'error' | 'warning' = 'info',
  durationMs = 3500,
) {
  const toast = $('global-toast');
  if (!toast) return;
  toast.textContent = message;
  toast.className = `global-toast toast-${type}`;
  toast.style.display = 'block';
  setTimeout(() => {
    toast.style.display = 'none';
  }, durationMs);
}
