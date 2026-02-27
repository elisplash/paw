#!/usr/bin/env node
// Auto-install prek pre-commit hooks on `npm install`.
// Skipped in CI (no .git directory) and production installs.

import { execSync } from "node:child_process";
import { existsSync, copyFileSync, chmodSync } from "node:fs";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const GIT_DIR = join(ROOT, ".git");

// Skip if not inside a git repo (CI tarball, production deploy, etc.)
if (!existsSync(GIT_DIR)) {
  console.log("No .git directory — skipping hook setup.");
  process.exit(0);
}

// ── 1. Ensure prek binary is available ───────────────────────────
function hasPrek() {
  try {
    execSync("prek --version", { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

if (!hasPrek()) {
  console.log("Installing prek (fast pre-commit hooks)…");
  try {
    execSync(
      "curl -LsSf https://github.com/j178/prek/releases/latest/download/prek-installer.sh | sh",
      { stdio: "inherit" },
    );
  } catch (err) {
    console.warn(
      "Could not auto-install prek. Install manually: https://prek.j178.dev/installation/",
    );
    process.exit(0); // non-fatal — don't block npm install
  }
}

// ── 2. Install the git hooks ─────────────────────────────────────
try {
  execSync("prek install", { stdio: "inherit", cwd: ROOT });
  console.log("Pre-commit hooks installed.");
} catch (err) {
  console.warn("prek install failed:", err.message);
  process.exit(0); // non-fatal
}

// ── 3. Install the pre-push hook ─────────────────────────────────
const prePushSrc = join(ROOT, "scripts", "pre-push");
const prePushDst = join(GIT_DIR, "hooks", "pre-push");

try {
  copyFileSync(prePushSrc, prePushDst);
  chmodSync(prePushDst, 0o755);
  console.log("Pre-push hook installed.");
} catch (err) {
  console.warn("Could not install pre-push hook:", err.message);
  // non-fatal
}
