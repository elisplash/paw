// Paw Workspace — File-based storage for user-visible data
// Saves research, content, and builds to ~/Documents/Paw (or custom location)

import { homeDir, join } from '@tauri-apps/api/path';
import { mkdir, writeTextFile, readTextFile, readDir, remove, exists } from '@tauri-apps/plugin-fs';

// ── Types ──────────────────────────────────────────────────────────────────

export interface ResearchSource {
  url: string;
  title: string;
  favicon?: string;
  credibility: number; // 1-5
  extractedAt: string;
  snippets: string[];
}

export interface ResearchFinding {
  id: string;
  query: string;
  created: string;
  updated: string;
  content: string;
  summary?: string;
  keyPoints: string[];
  sources: ResearchSource[];
  tags: string[];
}

export interface ResearchProject {
  id: string;
  name: string;
  description?: string;
  created: string;
  updated: string;
  queries: string[]; // history of research queries
}

export interface ResearchReport {
  id: string;
  title: string;
  created: string;
  content: string;
  findingIds: string[];
}

// ── Workspace Path Management ──────────────────────────────────────────────

let workspacePath: string | null = null;

export async function getWorkspacePath(): Promise<string> {
  if (workspacePath) return workspacePath;

  // Check localStorage for custom path
  const customPath = localStorage.getItem('paw-workspace-path');
  if (customPath) {
    workspacePath = customPath;
    return workspacePath;
  }

  // Default to ~/Documents/Paw
  const home = await homeDir();
  workspacePath = await join(home, 'Documents', 'Paw');
  return workspacePath;
}

export function setWorkspacePath(path: string) {
  workspacePath = path;
  localStorage.setItem('paw-workspace-path', path);
}

export async function ensureWorkspace(): Promise<void> {
  const base = await getWorkspacePath();

  // Create directory structure
  const dirs = [
    base,
    await join(base, 'Research'),
    await join(base, 'Content'),
    await join(base, 'Build'),
    await join(base, 'exports'),
  ];

  for (const dir of dirs) {
    try {
      const dirExists = await exists(dir);
      if (!dirExists) {
        await mkdir(dir, { recursive: true });
      }
    } catch (e) {
      console.warn(`[workspace] Could not create ${dir}:`, e);
    }
  }
}

// ── Research Project Operations ────────────────────────────────────────────

export async function getResearchProjectPath(projectId: string): Promise<string> {
  const base = await getWorkspacePath();
  return join(base, 'Research', projectId);
}

export async function listResearchProjects(): Promise<ResearchProject[]> {
  const base = await getWorkspacePath();
  const researchDir = await join(base, 'Research');

  try {
    const entries = await readDir(researchDir);
    const projects: ResearchProject[] = [];

    for (const entry of entries) {
      if (entry.isDirectory && entry.name) {
        try {
          const projectPath = await join(researchDir, entry.name, 'project.json');
          const content = await readTextFile(projectPath);
          projects.push(JSON.parse(content));
        } catch {
          // Skip invalid project folders
        }
      }
    }

    return projects.sort((a, b) => new Date(b.updated).getTime() - new Date(a.updated).getTime());
  } catch {
    return [];
  }
}

export async function createResearchProject(
  name: string,
  description?: string,
): Promise<ResearchProject> {
  const id = generateId();
  const now = new Date().toISOString();

  const project: ResearchProject = {
    id,
    name,
    description,
    created: now,
    updated: now,
    queries: [],
  };

  const projectDir = await getResearchProjectPath(id);
  await mkdir(projectDir, { recursive: true });
  await mkdir(await join(projectDir, 'findings'), { recursive: true });
  await mkdir(await join(projectDir, 'reports'), { recursive: true });

  await writeTextFile(await join(projectDir, 'project.json'), JSON.stringify(project, null, 2));

  // Also create a README for the folder
  await writeTextFile(
    await join(projectDir, 'README.md'),
    `# ${name}\n\n${description || ''}\n\nCreated: ${new Date().toLocaleDateString()}\n`,
  );

  return project;
}

export async function getResearchProject(projectId: string): Promise<ResearchProject | null> {
  try {
    const projectPath = await join(await getResearchProjectPath(projectId), 'project.json');
    const content = await readTextFile(projectPath);
    return JSON.parse(content);
  } catch {
    return null;
  }
}

export async function updateResearchProject(project: ResearchProject): Promise<void> {
  project.updated = new Date().toISOString();
  const projectPath = await join(await getResearchProjectPath(project.id), 'project.json');
  await writeTextFile(projectPath, JSON.stringify(project, null, 2));
}

export async function deleteResearchProject(projectId: string): Promise<void> {
  const projectDir = await getResearchProjectPath(projectId);
  await remove(projectDir, { recursive: true });
}

// ── Research Finding Operations ────────────────────────────────────────────

export async function listFindings(projectId: string): Promise<ResearchFinding[]> {
  const projectDir = await getResearchProjectPath(projectId);
  const findingsDir = await join(projectDir, 'findings');

  try {
    const entries = await readDir(findingsDir);
    const findings: ResearchFinding[] = [];

    for (const entry of entries) {
      if (entry.name?.endsWith('.json')) {
        try {
          const filePath = await join(findingsDir, entry.name);
          const content = await readTextFile(filePath);
          findings.push(JSON.parse(content));
        } catch {
          // Skip invalid files
        }
      }
    }

    return findings.sort((a, b) => new Date(b.created).getTime() - new Date(a.created).getTime());
  } catch {
    return [];
  }
}

export async function saveFinding(projectId: string, finding: ResearchFinding): Promise<void> {
  const projectDir = await getResearchProjectPath(projectId);
  const findingsDir = await join(projectDir, 'findings');

  // Save as JSON (machine-readable)
  await writeTextFile(
    await join(findingsDir, `${finding.id}.json`),
    JSON.stringify(finding, null, 2),
  );

  // Also save as Markdown (human-readable)
  const markdown = findingToMarkdown(finding);
  await writeTextFile(await join(findingsDir, `${finding.id}.md`), markdown);

  // Update project timestamp
  const project = await getResearchProject(projectId);
  if (project) {
    if (!project.queries.includes(finding.query)) {
      project.queries.push(finding.query);
    }
    await updateResearchProject(project);
  }
}

export async function getFinding(
  projectId: string,
  findingId: string,
): Promise<ResearchFinding | null> {
  try {
    const projectDir = await getResearchProjectPath(projectId);
    const filePath = await join(projectDir, 'findings', `${findingId}.json`);
    const content = await readTextFile(filePath);
    return JSON.parse(content);
  } catch {
    return null;
  }
}

export async function deleteFinding(projectId: string, findingId: string): Promise<void> {
  const projectDir = await getResearchProjectPath(projectId);
  const findingsDir = await join(projectDir, 'findings');

  try {
    await remove(await join(findingsDir, `${findingId}.json`));
    await remove(await join(findingsDir, `${findingId}.md`));
  } catch {
    // Ignore if files don't exist
  }
}

// ── Research Report Operations ─────────────────────────────────────────────

export async function listReports(projectId: string): Promise<ResearchReport[]> {
  const projectDir = await getResearchProjectPath(projectId);
  const reportsDir = await join(projectDir, 'reports');

  try {
    const entries = await readDir(reportsDir);
    const reports: ResearchReport[] = [];

    for (const entry of entries) {
      if (entry.name?.endsWith('.json')) {
        try {
          const filePath = await join(reportsDir, entry.name);
          const content = await readTextFile(filePath);
          reports.push(JSON.parse(content));
        } catch {
          // Skip invalid files
        }
      }
    }

    return reports.sort((a, b) => new Date(b.created).getTime() - new Date(a.created).getTime());
  } catch {
    return [];
  }
}

export async function saveReport(projectId: string, report: ResearchReport): Promise<void> {
  const projectDir = await getResearchProjectPath(projectId);
  const reportsDir = await join(projectDir, 'reports');

  // Save as JSON
  await writeTextFile(await join(reportsDir, `${report.id}.json`), JSON.stringify(report, null, 2));

  // Save as Markdown
  await writeTextFile(await join(reportsDir, `${report.id}.md`), report.content);
}

// ── Sources File ───────────────────────────────────────────────────────────

export async function getAllSources(projectId: string): Promise<ResearchSource[]> {
  const findings = await listFindings(projectId);
  const sourcesMap = new Map<string, ResearchSource>();

  for (const finding of findings) {
    for (const source of finding.sources) {
      const existing = sourcesMap.get(source.url);
      if (existing) {
        // Merge snippets, keep higher credibility
        existing.snippets = [...new Set([...existing.snippets, ...source.snippets])];
        existing.credibility = Math.max(existing.credibility, source.credibility);
      } else {
        sourcesMap.set(source.url, { ...source });
      }
    }
  }

  return Array.from(sourcesMap.values()).sort((a, b) => b.credibility - a.credibility);
}

// ── Markdown Conversion ────────────────────────────────────────────────────

function findingToMarkdown(finding: ResearchFinding): string {
  const frontmatter = [
    '---',
    `id: ${finding.id}`,
    `query: "${finding.query.replace(/"/g, '\\"')}"`,
    `created: ${finding.created}`,
    `updated: ${finding.updated}`,
    `tags: [${finding.tags.map((t) => `"${t}"`).join(', ')}]`,
    'sources:',
    ...finding.sources.map(
      (s) =>
        `  - url: "${s.url}"\n    title: "${s.title.replace(/"/g, '\\"')}"\n    credibility: ${s.credibility}`,
    ),
    '---',
    '',
  ].join('\n');

  const keyPointsSection =
    finding.keyPoints.length > 0
      ? `## Key Points\n\n${finding.keyPoints.map((p) => `- ${p}`).join('\n')}\n\n`
      : '';

  const sourcesSection = `## Sources\n\n${finding.sources
    .map(
      (s) =>
        `- [${s.title}](${s.url}) ${'●'.repeat(s.credibility)}${'○'.repeat(5 - s.credibility)}`,
    )
    .join('\n')}\n`;

  return `${frontmatter}# ${finding.query}\n\n${finding.summary ? `> ${finding.summary}\n\n` : ''}${keyPointsSection}## Details\n\n${finding.content}\n\n${sourcesSection}`;
}

// ── Utilities ──────────────────────────────────────────────────────────────

function generateId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function generateFindingId(): string {
  return generateId();
}

// ── Parse Agent Response into Structured Finding ───────────────────────────

export function parseAgentResponse(
  query: string,
  rawContent: string,
  extractedSources: ResearchSource[],
): Omit<ResearchFinding, 'id' | 'created' | 'updated'> {
  // Extract key points (lines starting with - or * or numbered)
  const keyPoints: string[] = [];
  const lines = rawContent.split('\n');

  for (const line of lines) {
    const trimmed = line.trim();
    if (/^[-•]\s+\*\*/.test(trimmed) || /^\d+\.\s+\*\*/.test(trimmed)) {
      // Bold bullet points are likely key findings
      const point = trimmed
        .replace(/^[-•\d.]+\s*/, '')
        .replace(/\*\*/g, '')
        .trim();
      if (point.length > 10 && point.length < 200) {
        keyPoints.push(point);
      }
    }
  }

  // Extract summary (first paragraph that's not a header)
  let summary = '';
  for (const line of lines) {
    const trimmed = line.trim();
    if (
      trimmed &&
      !trimmed.startsWith('#') &&
      !trimmed.startsWith('-') &&
      !trimmed.startsWith('•')
    ) {
      if (trimmed.length > 50 && trimmed.length < 500) {
        summary = trimmed;
        break;
      }
    }
  }

  // Extract tags from content (look for common patterns)
  const tags: string[] = [];
  const tagPatterns = [
    /\b(performance|optimization|security|best practice|tip|warning|gotcha)\b/gi,
  ];
  for (const pattern of tagPatterns) {
    const matches = rawContent.match(pattern);
    if (matches) {
      tags.push(...matches.map((m) => m.toLowerCase()));
    }
  }

  return {
    query,
    content: rawContent,
    summary: summary || undefined,
    keyPoints: keyPoints.slice(0, 5), // Top 5 key points
    sources: extractedSources,
    tags: [...new Set(tags)].slice(0, 5),
  };
}

// ── Open in Finder ─────────────────────────────────────────────────────────

export async function openInFinder(projectId?: string): Promise<void> {
  const { open } = await import('@tauri-apps/plugin-shell');

  let path: string;
  if (projectId) {
    path = await getResearchProjectPath(projectId);
  } else {
    path = await getWorkspacePath();
  }

  await open(path);
}
