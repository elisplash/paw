// ─────────────────────────────────────────────────────────────────────────────
// Flow Parser — Tests
// ─────────────────────────────────────────────────────────────────────────────

import { describe, it, expect } from 'vitest';
import { parseFlowText } from './parser';

describe('parseFlowText', () => {
  // ── Arrow syntax ─────────────────────────────────────────────────────────

  describe('arrow syntax', () => {
    it('parses simple arrow chain', () => {
      const { graph } = parseFlowText('webhook → agent → send email');
      expect(graph.nodes).toHaveLength(3);
      expect(graph.edges).toHaveLength(2);
      expect(graph.nodes[0].kind).toBe('trigger'); // webhook
      expect(graph.nodes[1].kind).toBe('agent'); // agent
      expect(graph.nodes[2].kind).toBe('output'); // send email
    });

    it('parses ASCII arrows (->)', () => {
      const { graph } = parseFlowText('start -> process -> end');
      expect(graph.nodes).toHaveLength(3);
      expect(graph.edges).toHaveLength(2);
    });

    it('parses fat arrows (=>)', () => {
      const { graph } = parseFlowText('trigger => tool => output');
      expect(graph.nodes).toHaveLength(3);
    });

    it('detects node kinds from keywords', () => {
      const { graph } = parseFlowText('webhook → AI agent → transform data → send email');
      expect(graph.nodes[0].kind).toBe('trigger');
      expect(graph.nodes[1].kind).toBe('agent');
      expect(graph.nodes[2].kind).toBe('data');
      expect(graph.nodes[3].kind).toBe('output');
    });

    it('supports multi-line arrow flows', () => {
      const { graph } = parseFlowText('webhook → agent\nagent → send email');
      expect(graph.nodes).toHaveLength(3);
      expect(graph.edges).toHaveLength(2);
    });
  });

  // ── Numbered list ────────────────────────────────────────────────────────

  describe('numbered list', () => {
    it('parses numbered steps', () => {
      const { graph } = parseFlowText('1. webhook\n2. agent\n3. send email');
      expect(graph.nodes).toHaveLength(3);
      expect(graph.edges).toHaveLength(2);
    });

    it('handles parenthesized numbers', () => {
      const { graph } = parseFlowText('1) listen for events\n2) AI processes\n3) log results');
      expect(graph.nodes).toHaveLength(3);
    });
  });

  // ── Pipe syntax ──────────────────────────────────────────────────────────

  describe('pipe syntax', () => {
    it('parses pipe-separated steps', () => {
      const { graph } = parseFlowText('webhook | agent | output');
      expect(graph.nodes).toHaveLength(3);
      expect(graph.edges).toHaveLength(2);
    });
  });

  // ── Prose ────────────────────────────────────────────────────────────────

  describe('prose', () => {
    it('parses comma+then sentences', () => {
      const { graph } = parseFlowText(
        'When a webhook fires, then the agent processes it, then send an email',
      );
      expect(graph.nodes.length).toBeGreaterThanOrEqual(3);
      expect(graph.edges.length).toBeGreaterThanOrEqual(2);
    });

    it('parses semicolon-separated prose', () => {
      const { graph } = parseFlowText('fetch data; transform it; save results');
      expect(graph.nodes).toHaveLength(3);
    });

    it('adds warning for unparseable text', () => {
      const { graph, warnings } = parseFlowText('hello');
      expect(graph.nodes).toHaveLength(1);
      expect(warnings.length).toBeGreaterThan(0);
    });
  });

  // ── Edge kinds ───────────────────────────────────────────────────────────

  describe('edge kinds', () => {
    it('detects reverse edge from keywords', () => {
      const { graph } = parseFlowText('1. agent\n2. pull from database');
      const edge = graph.edges[0];
      expect(edge.kind).toBe('reverse');
    });

    it('detects bidirectional edge from keywords', () => {
      const { graph } = parseFlowText('1. agent\n2. sync with server');
      const edge = graph.edges[0];
      expect(edge.kind).toBe('bidirectional');
    });
  });

  // ── Edge cases ───────────────────────────────────────────────────────────

  describe('edge cases', () => {
    it('handles empty input', () => {
      const { graph, warnings } = parseFlowText('');
      expect(graph.nodes).toHaveLength(0);
      expect(warnings).toContain('Empty input');
    });

    it('deduplicates nodes with same label', () => {
      const { graph } = parseFlowText('agent → tool → agent');
      expect(graph.nodes).toHaveLength(2); // agent reused
      expect(graph.edges).toHaveLength(2);
    });

    it('applies layout (nodes have positive positions)', () => {
      const { graph } = parseFlowText('A → B → C');
      expect(graph.nodes[0].x).toBeGreaterThanOrEqual(0);
      expect(graph.nodes[0].y).toBeGreaterThanOrEqual(0);
    });
  });
});
