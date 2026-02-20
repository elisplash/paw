---
sidebar_position: 6
title: Research
---

# Research

The Research view lets agents conduct structured web research with source tracking and credibility scoring.

## Modes

| Mode | Sources | Timeout | When to use |
|------|---------|---------|-------------|
| **Quick** | 3–5 | 120s | Fast answers, simple questions |
| **Deep** | 10+ | 300s | Thorough investigation, complex topics |

## Workflow

1. Enter a research query
2. Choose **Quick** or **Deep** mode
3. Watch the live progress:
   - **Searching** — finding sources
   - **Reading** — fetching pages
   - **Analyzing** — processing content
   - **Found** — result discovered
   - **Summarizing** — generating synthesis
4. Review findings with sources

## Findings

Each research finding includes:

- **Summary** — key takeaway
- **Content** — full extracted text
- **Key points** — bullet-point highlights
- **Sources** — URLs with titles and credibility scores (1–5)

### Source credibility

Sources are scored on a 1–5 scale shown as dots:
- ●●●●● — Highly credible (academic, official docs)
- ●●●●○ — Generally reliable
- ●●●○○ — Mixed reliability
- ●●○○○ — Use with caution
- ●○○○○ — Unreliable

## Actions

For each finding you can:
- **Dig Deeper** — ask follow-up questions about this finding
- **Find Related** — search for related topics
- **View Full** — see the complete content
- **Delete** — remove the finding

## Reports

Generate a compiled report from your findings:
- Saved as markdown to `~/Documents/Paw/Research/`
- Includes all findings, sources, and key points
- Per-project sessions with query history

## Tips

- Start with a broad quick search, then dig deeper on interesting findings
- Use the source credibility scores to prioritize reliable information
- Chain findings together: dig deeper → find related → synthesize
