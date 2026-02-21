// Orchestrator View — Pure helpers and types (no DOM, no IPC)

export function specialtyIcon(specialty: string): string {
  const icons: Record<string, string> = {
    coder: 'code',
    researcher: 'search',
    designer: 'palette',
    communicator: 'campaign',
    security: 'shield',
    general: 'smart_toy',
  };
  const name = icons[specialty] || 'smart_toy';
  return `<span class="ms ms-sm">${name}</span>`;
}

export function messageKindLabel(kind: string): string {
  const labels: Record<string, string> = {
    delegation: 'Delegation',
    progress: 'Progress',
    result: 'Result',
    error: 'Error',
    message: 'Message',
  };
  return labels[kind] || kind;
}

export function formatTime(dateStr: string): string {
  try {
    const d = new Date(dateStr);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  } catch {
    return dateStr;
  }
}
