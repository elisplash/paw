// src/components/kinetic-row.ts — VST-grade motion system
//
// Vanilla TS utility that applies breathing indicators, signal waves,
// spring hover, and warm-up materialisation to list/card elements.
// No React — pure DOM manipulation with CSS class orchestration.

// ── Types ──────────────────────────────────────────────────────────────

export type KineticStatus = 'healthy' | 'warning' | 'error' | 'idle';

export type SignalColor = 'accent' | 'success' | 'warning' | 'error';

export interface KineticRowOptions {
  /** Apply breathing indicator dot */
  breathe?: boolean;
  /** Status controls dot color + oscillation */
  status?: KineticStatus;
  /** Apply spring hover effect */
  spring?: boolean;
  /** Use stronger spring (for cards) */
  springCard?: boolean;
  /** Apply materialise entrance animation */
  materialise?: boolean;
  /** Apply border oscillation for connected items */
  oscillate?: boolean;
  /** Apply halftone pulse capability */
  halftone?: boolean;
}

// ── Core API ───────────────────────────────────────────────────────────

/**
 * Apply kinetic behaviors to a DOM element.
 * Returns a controller object for triggering signals and cleanup.
 */
export function kineticRow(el: HTMLElement, opts: KineticRowOptions = {}) {
  // Base class — enables signal wave ::before pseudo
  el.classList.add('k-row');

  // Breathing indicator
  if (opts.breathe) {
    el.classList.add('k-breathe');
  }

  // Status (affects dot color + oscillation color)
  if (opts.status) {
    el.classList.add(`k-status-${opts.status}`);
  }

  // Spring hover
  if (opts.springCard) {
    el.classList.add('k-spring-card');
  } else if (opts.spring) {
    el.classList.add('k-spring');
  }

  // Materialise entrance
  if (opts.materialise) {
    el.classList.add('k-materialise');
  }

  // Border oscillation
  if (opts.oscillate && opts.status !== 'idle') {
    el.classList.add('k-oscillate');
  }

  // Halftone pulse
  if (opts.halftone) {
    el.classList.add('k-halftone-pulse');
  }

  // ── Controller ─────────────────────────────────────────────────

  return {
    /**
     * Fire a signal wave across the element.
     * The wave animates once then resets automatically.
     */
    signal(color: SignalColor = 'accent') {
      // Remove existing signal classes
      el.classList.remove(
        'k-signal-fire',
        'k-signal-accent',
        'k-signal-success',
        'k-signal-warning',
        'k-signal-error',
      );
      // Force reflow to restart animation
      void el.offsetWidth;
      el.classList.add(`k-signal-${color}`, 'k-signal-fire');

      // Auto-cleanup after animation
      const onEnd = () => {
        el.classList.remove('k-signal-fire', `k-signal-${color}`);
        el.removeEventListener('animationend', onEnd);
      };
      el.addEventListener('animationend', onEnd, { once: false });

      // Fallback cleanup in case animationend doesn't fire
      setTimeout(() => {
        el.classList.remove('k-signal-fire', `k-signal-${color}`);
      }, 1000);
    },

    /** Fire a halftone pulse effect */
    halftone() {
      el.classList.remove('k-halftone-fire');
      void el.offsetWidth;
      el.classList.add('k-halftone-fire');
      setTimeout(() => el.classList.remove('k-halftone-fire'), 1400);
    },

    /** Update the status (changes breathing dot color + oscillation) */
    setStatus(status: KineticStatus) {
      el.classList.remove(
        'k-status-healthy',
        'k-status-warning',
        'k-status-error',
        'k-status-idle',
      );
      el.classList.add(`k-status-${status}`);

      if (status === 'idle') {
        el.classList.remove('k-oscillate');
      } else if (opts.oscillate) {
        el.classList.add('k-oscillate');
      }
    },

    /** Remove all kinetic classes */
    destroy() {
      el.classList.remove(
        'k-row',
        'k-breathe',
        'k-spring',
        'k-spring-card',
        'k-materialise',
        'k-oscillate',
        'k-halftone-pulse',
        'k-signal-fire',
        'k-signal-accent',
        'k-signal-success',
        'k-signal-warning',
        'k-signal-error',
        'k-halftone-fire',
        'k-status-healthy',
        'k-status-warning',
        'k-status-error',
        'k-status-idle',
      );
    },
  };
}

// ── Convenience: Apply staggered materialise to a container's children ──

/**
 * Add `.k-stagger` to a parent and `.k-materialise` to each child.
 * Children will animate in with staggered delays (defined in CSS).
 */
export function kineticStagger(parent: HTMLElement, childSelector?: string) {
  parent.classList.add('k-stagger');
  const children = childSelector ? parent.querySelectorAll(childSelector) : parent.children;

  for (const child of children) {
    (child as HTMLElement).classList.add('k-materialise');
  }
}

// ── Convenience: Create a breathing indicator dot element ──

/**
 * Create a `<span class="k-indicator">` element for use inside k-breathe rows.
 */
export function kineticDot(): string {
  return '<span class="k-indicator"></span>';
}

// ── Signal Bus — fire signals across multiple rows by selector ──

const _controllers = new Map<HTMLElement, ReturnType<typeof kineticRow>>();

/**
 * Register a kinetic controller for event-driven signals.
 * Use with the signal bus to fire waves across specific elements.
 */
export function registerKinetic(id: string, el: HTMLElement, ctrl: ReturnType<typeof kineticRow>) {
  el.dataset.kineticId = id;
  _controllers.set(el, ctrl);
}

/**
 * Fire a signal wave on all registered kinetic elements matching a selector/ID.
 * Useful for broadcasting events (e.g., new message → flash that channel's row).
 */
export function broadcastSignal(kineticId: string, color: SignalColor = 'accent') {
  for (const [el, ctrl] of _controllers) {
    if (el.dataset.kineticId === kineticId) {
      ctrl.signal(color);
    }
  }
}

/**
 * Fire a signal wave on all registered kinetic elements.
 * Use sparingly — for global events like connection/disconnection.
 */
export function broadcastAll(color: SignalColor = 'accent') {
  for (const [, ctrl] of _controllers) {
    ctrl.signal(color);
  }
}
