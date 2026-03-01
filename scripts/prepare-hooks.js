#!/usr/bin/env node
// Auto-install prek git hooks on `npm install`.
// Copies scripts/pre-commit and scripts/pre-push into .git/hooks/.
// Each hook auto-installs prek at runtime if it's missing, so even a
// fresh clone without prek on PATH will "just work" on first commit/push.
//
// Skipped in CI (no .git directory) and production installs.

import { existsSync, copyFileSync, chmodSync } from "node:fs";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const GIT_DIR = join(ROOT, ".git");

// Skip if not inside a git repo (CI tarball, production deploy, etc.)
if (!existsSync(GIT_DIR)) {
  console.log("No .git directory — skipping hook setup.");
  process.exit(0);
}

// ── Install hooks from scripts/ → .git/hooks/ ───────────────────
const hooks = ["pre-commit", "pre-push"];

for (const hook of hooks) {
  const src = join(ROOT, "scripts", hook);
  const dst = join(GIT_DIR, "hooks", hook);

  if (!existsSync(src)) {
    console.warn(`scripts/${hook} not found — skipping.`);
    continue;
  }

  try {
    copyFileSync(src, dst);
    chmodSync(dst, 0o755);
    console.log(`${hook} hook installed.`);
  } catch (err) {
    console.warn(`Could not install ${hook} hook:`, err.message);
    // non-fatal — don't block npm install
  }
}
