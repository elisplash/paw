// Pawz — Features Directory
//
// Atomic design pattern:
//   atoms/     — Pure functions, single-responsibility (parsers, validators, formatters)
//   molecules/ — Composed atoms (e.g. slash command parser + executor)
//   index.ts   — Public API for the feature
//
// Each feature is self-contained. No new code in main.ts.
// main.ts only imports and wires features via their public API.

export {};
