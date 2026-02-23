// Mail View — Atoms (pure logic, types, constants)
// Zero DOM, zero IPC imports

// ── Types ──────────────────────────────────────────────────────────────────

export interface MailPermissions {
  read: boolean;
  send: boolean;
  delete: boolean;
  manage: boolean;
}

export interface MailMessage {
  id: string;
  from: string;
  subject: string;
  snippet: string;
  date: Date;
  body?: string;
  sessionKey?: string;
  read?: boolean;
  /** 'himalaya' (IMAP) or 'google' (Gmail API) */
  source?: 'himalaya' | 'google';
}

export interface MailAccount {
  name: string;
  email: string;
}

// ── Provider presets ───────────────────────────────────────────────────────

export const EMAIL_PROVIDERS: Record<
  string,
  {
    name: string;
    icon: string;
    imap: string;
    imapPort: number;
    smtp: string;
    smtpPort: number;
    hint: string;
  }
> = {
  gmail: {
    name: 'Gmail',
    icon: 'G',
    imap: 'imap.gmail.com',
    imapPort: 993,
    smtp: 'smtp.gmail.com',
    smtpPort: 465,
    hint: 'Use an App Password — go to Google Account → Security → App Passwords',
  },
  outlook: {
    name: 'Outlook / Hotmail',
    icon: 'O',
    imap: 'outlook.office365.com',
    imapPort: 993,
    smtp: 'smtp.office365.com',
    smtpPort: 587,
    hint: 'Use your regular password, or an App Password if 2FA is on',
  },
  yahoo: {
    name: 'Yahoo Mail',
    icon: 'Y',
    imap: 'imap.mail.yahoo.com',
    imapPort: 993,
    smtp: 'smtp.mail.yahoo.com',
    smtpPort: 465,
    hint: 'Generate an App Password in Yahoo Account Settings → Security',
  },
  icloud: {
    name: 'iCloud Mail',
    icon: 'iC',
    imap: 'imap.mail.me.com',
    imapPort: 993,
    smtp: 'smtp.mail.me.com',
    smtpPort: 587,
    hint: 'Use an App-Specific Password from appleid.apple.com',
  },
  fastmail: {
    name: 'Fastmail',
    icon: 'FM',
    imap: 'imap.fastmail.com',
    imapPort: 993,
    smtp: 'smtp.fastmail.com',
    smtpPort: 465,
    hint: 'Use an App Password from Settings → Privacy & Security',
  },
  custom: {
    name: 'Other (IMAP/SMTP)',
    icon: '*',
    imap: '',
    imapPort: 993,
    smtp: '',
    smtpPort: 465,
    hint: 'Enter your mail server details manually',
  },
};

// ── Content extraction ─────────────────────────────────────────────────────

export function extractContent(content: unknown): string {
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) {
    return content.map((c) => (c as { text?: string }).text ?? '').join('');
  }
  return '';
}

// ── Date formatter ─────────────────────────────────────────────────────────

export function formatMailDate(date: Date): string {
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  if (diff < 86400000 && now.getDate() === date.getDate()) {
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }
  if (diff < 604800000) {
    return date.toLocaleDateString([], { weekday: 'short' });
  }
  return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
}

// ── String helpers ─────────────────────────────────────────────────────────

export function getAvatarClass(sender: string): string {
  const colors = ['', 'avatar-green', 'avatar-purple', 'avatar-orange', 'avatar-pink'];
  if (sender.toLowerCase().includes('google')) return 'avatar-google';
  if (sender.toLowerCase().includes('microsoft')) return 'avatar-microsoft';
  const hash = sender.split('').reduce((acc, c) => acc + c.charCodeAt(0), 0);
  return colors[hash % colors.length];
}

export function getInitials(sender: string): string {
  const name = sender.replace(/<.*>/, '').trim();
  if (!name) return '?';
  const parts = name.split(/[\s@._-]+/).filter((p) => p.length > 0);
  if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
  return name.slice(0, 2).toUpperCase();
}

// ── Permissions (localStorage) ─────────────────────────────────────────────

export function loadMailPermissions(accountName: string): MailPermissions {
  try {
    const raw = localStorage.getItem(`mail-perms-${accountName}`);
    if (raw) return JSON.parse(raw);
  } catch {
    /* ignore */
  }
  return { read: true, send: true, delete: false, manage: false };
}

export function saveMailPermissions(accountName: string, perms: MailPermissions): void {
  localStorage.setItem(`mail-perms-${accountName}`, JSON.stringify(perms));
}

export function removeMailPermissions(accountName: string): void {
  localStorage.removeItem(`mail-perms-${accountName}`);
}
