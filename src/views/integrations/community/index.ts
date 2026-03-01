// src/views/integrations/community/index.ts — Orchestration + public API
//
// Thin barrel: re-exports the mount function + package requirement helpers.

export { mountCommunityBrowser, unmountCommunityBrowser } from './molecules';
export { getRequiredPackage, displayName, COMMUNITY_PACKAGE_MAP } from './atoms';
export type { CommunityPackage, InstalledPackage } from './atoms';
