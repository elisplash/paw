// ─────────────────────────────────────────────────────────────────────────────
// OpenPawz — Animation Utilities (powered by anime.js v4)
// Vanilla TS animation primitives for views, cards, toasts, and interactions.
// ─────────────────────────────────────────────────────────────────────────────

import { animate, stagger, createTimeline, spring, utils } from 'animejs';

// ── View Transitions ────────────────────────────────────────────────────────

/**
 * Animate a view entering the viewport. Call after setting display/active.
 * Cinematic fade + subtle upscale.
 */
export function viewEnter(el: HTMLElement | string) {
  return animate(el, {
    opacity: [0, 1],
    scale: [0.97, 1],
    translateY: [8, 0],
    duration: 350,
    ease: 'out(3)',
  });
}

/**
 * Animate a view leaving. Returns a promise — hide element after completion.
 */
export function viewLeave(el: HTMLElement | string) {
  return animate(el, {
    opacity: [1, 0],
    scale: [1, 0.97],
    translateY: [0, -8],
    duration: 250,
    ease: 'in(2)',
  });
}

// ── Card / List Stagger ─────────────────────────────────────────────────────

/**
 * Stagger-animate a set of child elements (cards, rows, list items).
 * Call after populating a container's innerHTML.
 */
export function staggerIn(selector: string, container?: HTMLElement) {
  const targets = container
    ? container.querySelectorAll(selector)
    : document.querySelectorAll(selector);
  if (!targets.length) return;

  // Start invisible then animate in
  targets.forEach((el) => {
    (el as HTMLElement).style.opacity = '0';
    (el as HTMLElement).style.transform = 'translateY(12px)';
  });

  return animate(targets as unknown as string, {
    opacity: [0, 1],
    translateY: [12, 0],
    delay: stagger(40, { start: 60 }),
    duration: 400,
    ease: 'out(3)',
  });
}

/**
 * Stagger-animate cards with a slight scale bounce — more dramatic entrance.
 */
export function staggerCards(selector: string, container?: HTMLElement) {
  const targets = container
    ? container.querySelectorAll(selector)
    : document.querySelectorAll(selector);
  if (!targets.length) return;

  targets.forEach((el) => {
    (el as HTMLElement).style.opacity = '0';
    (el as HTMLElement).style.transform = 'scale(0.92) translateY(16px)';
  });

  return animate(targets as unknown as string, {
    opacity: [0, 1],
    scale: [0.92, 1],
    translateY: [16, 0],
    delay: stagger(50, { start: 80 }),
    duration: 500,
    ease: spring({ stiffness: 200, damping: 18 }),
  });
}

// ── Toast Notifications ─────────────────────────────────────────────────────

/**
 * Animate a toast notification sliding in from the bottom.
 */
export function toastEnter(el: HTMLElement) {
  return animate(el, {
    translateX: ['-50%', '-50%'],
    translateY: [30, 0],
    opacity: [0, 1],
    duration: 400,
    ease: spring({ stiffness: 300, damping: 20 }),
  });
}

/**
 * Animate toast sliding out. Returns a promise for cleanup.
 */
export function toastLeave(el: HTMLElement) {
  return animate(el, {
    translateY: [0, 20],
    opacity: [1, 0],
    duration: 300,
    ease: 'in(2)',
  });
}

// ── Lock Screen ─────────────────────────────────────────────────────────────

/**
 * Dramatic lock screen unlock sequence — scale + blur + dissolve.
 */
export function lockScreenUnlock(lockEl: HTMLElement) {
  const inner = lockEl.querySelector('.lock-screen-inner') as HTMLElement | null;

  const tl = createTimeline({
    defaults: { ease: 'out(3)' },
  });

  // Phase 1: Logo burst
  const logo = lockEl.querySelector('.lock-logo') as HTMLElement | null;
  if (logo) {
    tl.add(
      logo,
      {
        scale: [1, 1.3, 0.9],
        opacity: [1, 0.6],
        duration: 400,
      },
      0,
    );
  }

  // Phase 2: Inner content slides up and fades
  if (inner) {
    tl.add(
      inner,
      {
        translateY: [0, -30],
        opacity: [1, 0],
        scale: [1, 1.04],
        duration: 500,
        ease: 'in(3)',
      },
      100,
    );
  }

  // Phase 3: Whole screen fades out
  tl.add(
    lockEl,
    {
      opacity: [1, 0],
      duration: 400,
      ease: 'in(2)',
    },
    350,
  );

  return tl;
}

/**
 * Shake an input on wrong password — spring-based jitter.
 */
export function shakeElement(el: HTMLElement) {
  return animate(el, {
    translateX: [0, -8, 7, -5, 4, -2, 0],
    duration: 500,
    ease: 'out(2)',
  });
}

/**
 * Lock screen initial entrance — logo glow + form fade-in.
 */
export function lockScreenEntrance(lockEl: HTMLElement) {
  const logo = lockEl.querySelector('.lock-logo-icon') as HTMLElement | null;
  const inner = lockEl.querySelector('.lock-screen-inner') as HTMLElement | null;

  const tl = createTimeline({
    defaults: { ease: 'out(3)' },
  });

  if (logo) {
    tl.add(
      logo,
      {
        scale: [0.5, 1],
        opacity: [0, 1],
        duration: 600,
      },
      0,
    );
  }

  if (inner) {
    tl.add(
      inner,
      {
        opacity: [0, 1],
        translateY: [20, 0],
        duration: 500,
      },
      200,
    );
  }

  return tl;
}

// ── Sidebar ─────────────────────────────────────────────────────────────────

/**
 * Animate sidebar nav items on initial load — cascading fade-in.
 */
export function sidebarNavEntrance(navSelector = '.nav-item') {
  return animate(navSelector, {
    opacity: [0, 1],
    translateX: [-12, 0],
    delay: stagger(30, { start: 100 }),
    duration: 350,
    ease: 'out(3)',
  });
}

/**
 * Pulse animation for notification badges.
 */
export function badgePulse(el: HTMLElement) {
  return animate(el, {
    scale: [1, 1.3, 1],
    duration: 500,
    ease: spring({ stiffness: 400, damping: 10 }),
  });
}

// ── Micro-interactions ──────────────────────────────────────────────────────

/**
 * Subtle press/click feedback for buttons.
 */
export function tapFeedback(el: HTMLElement) {
  return animate(el, {
    scale: [1, 0.95, 1],
    duration: 200,
    ease: 'out(2)',
  });
}

/**
 * Expand/reveal an element (e.g., drawer, panel).
 */
export function expandPanel(el: HTMLElement, fromHeight = 0) {
  const fullHeight = el.scrollHeight;
  el.style.overflow = 'hidden';
  return animate(el, {
    height: [fromHeight, fullHeight],
    opacity: [0.5, 1],
    duration: 400,
    ease: 'out(3)',
    onComplete: () => {
      el.style.height = 'auto';
      el.style.overflow = '';
    },
  });
}

/**
 * Collapse/hide a panel.
 */
export function collapsePanel(el: HTMLElement) {
  el.style.height = `${el.scrollHeight}px`;
  el.style.overflow = 'hidden';
  return animate(el, {
    height: [el.scrollHeight, 0],
    opacity: [1, 0],
    duration: 300,
    ease: 'in(2)',
    onComplete: () => {
      el.style.display = 'none';
      el.style.overflow = '';
    },
  });
}

/**
 * Smooth counter/number roll (for dashboard stats).
 */
export function rollNumber(
  el: HTMLElement,
  from: number,
  to: number,
  duration = 800,
  formatFn?: (v: number) => string,
) {
  const obj = { value: from };
  return animate(obj, {
    value: [from, to],
    duration,
    ease: 'out(3)',
    onUpdate: () => {
      el.textContent = formatFn ? formatFn(Math.round(obj.value)) : String(Math.round(obj.value));
    },
  });
}

/**
 * Attention-grabbing bounce for an element (e.g., agent avatar).
 */
export function attentionBounce(el: HTMLElement) {
  return animate(el, {
    translateY: [0, -8, 0],
    scale: [1, 1.05, 1],
    duration: 600,
    ease: spring({ stiffness: 300, damping: 12 }),
  });
}

/**
 * Morphing glow effect — cycles through accent colours.
 */
export function glowPulse(el: HTMLElement, color = 'rgba(255, 77, 77, 0.6)') {
  return animate(el, {
    boxShadow: [
      `0 0 0px ${color.replace(/[\d.]+\)/, '0)')}`,
      `0 0 16px ${color}`,
      `0 0 0px ${color.replace(/[\d.]+\)/, '0)')}`,
    ],
    duration: 2000,
    loop: true,
    ease: 'inOut(2)',
  });
}

/**
 * Flow node materialisation — scales in with a kinetic ripple.
 */
export function flowNodeMaterialise(el: HTMLElement) {
  return animate(el, {
    scale: [0, 1],
    opacity: [0, 1],
    rotate: ['-5deg', '0deg'],
    duration: 500,
    ease: spring({ stiffness: 260, damping: 15 }),
  });
}

/**
 * Animate a progress bar fill.
 */
export function progressFill(el: HTMLElement, toPercent: number, duration = 600) {
  return animate(el, {
    width: [`0%`, `${toPercent}%`],
    duration,
    ease: 'out(3)',
  });
}

// ── Re-export anime.js primitives for advanced use ──────────────────────────

export { animate, stagger, createTimeline, spring, utils };
