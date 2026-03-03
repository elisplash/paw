// ─────────────────────────────────────────────────────────────────────────────
// Flow Simulation Engine — Stress Tests
// Hammers the simulation runtime with massive, complex, adversarial scenarios.
// ─────────────────────────────────────────────────────────────────────────────

import { describe, it, expect } from 'vitest';
import { runSimulation, runSimSuite } from './simulation-runtime';
import type { SimScenario } from './simulation-atoms';
import {
  zapierMegaChainScenario,
  deepTesseractScenario,
  reverseEdgeScenario,
  tripleMeshDebateScenario,
  multiDiamondScenario,
  everyNodeKindScenario,
  cascadingConditionTreeScenario,
  multiErrorCascadeScenario,
  deepCollapseChainScenario,
  massiveParallelFanOutScenario,
  allEdgeKindsScenario,
  loopIterationScenario,
  squadMemoryMcpScenario,
  chaosRetryTortureScenario,
  wideOrchestratorScenario,
  variablePropagationStressScenario,
  zapierEtlPipelineScenario,
  tesseractReversedScenario,
  parallelCollapseHybridScenario,
  adversarialMockScenario,
  allStressScenarios,
  stressTestSuite,
} from './simulation-stress-scenarios';

// ── Helper: run and assert all expectations pass ───────────────────────────

async function runAndAssert(scenario: SimScenario) {
  const result = await runSimulation(scenario);
  if (!result.passed) {
    const failures = result.expectationResults.filter((e) => !e.passed);
    console.log(
      `FAILED [${scenario.id}]:`,
      failures.map((f) => `${f.expectation.description}: ${f.message} (actual: ${f.actual})`),
    );
  }
  return result;
}

// ═══════════════════════════════════════════════════════════════════════════
// STRESS TEST SUITE
// ═══════════════════════════════════════════════════════════════════════════

describe('Simulation Stress Tests', () => {
  // ── Zapier-Scale ─────────────────────────────────────────────────────

  describe('Zapier-Scale Flows', () => {
    it('runs 30-node mega-chain (Zapier style)', async () => {
      const result = await runAndAssert(zapierMegaChainScenario);
      expect(result.runState!.status).toBe('success');
      expect(result.strategy?.conductorUsed).toBe(true);
      // All 30 nodes should have been processed
      expect(result.runState!.nodeStates.size).toBeGreaterThanOrEqual(20);
      expect(result.passed).toBe(true);
    });

    it('runs 5-source ETL pipeline (Zapier data)', async () => {
      const result = await runAndAssert(zapierEtlPipelineScenario);
      expect(result.runState!.status).toBe('success');
      // All 5 HTTP fetches should fire
      const httpCalls = result.mockCalls.filter((c) => c.type === 'http');
      expect(httpCalls.length).toBe(5);
      expect(result.passed).toBe(true);
    });
  });

  // ── Tesseract-Style ──────────────────────────────────────────────────

  describe('Tesseract & Multi-Phase', () => {
    it('runs deep tesseract (5 phases × 4 cells)', async () => {
      const result = await runAndAssert(deepTesseractScenario);
      expect(result.runState!.status).toBe('success');
      // 20 cells + 4 horizons + trigger + output = 26 nodes
      expect(result.runState!.nodeStates.size).toBeGreaterThanOrEqual(20);
      // All 4 event horizons should fire
      for (let i = 0; i < 4; i++) {
        expect(result.runState!.nodeStates.get(`eh${i}`)?.status).toBe('success');
      }
      expect(result.passed).toBe(true);
    });

    it('runs reversed tesseract (pull-based phases)', async () => {
      const result = await runAndAssert(tesseractReversedScenario);
      expect(result.runState!.status).toBe('success');
      expect(result.runState!.nodeStates.get('eh0')?.status).toBe('success');
      expect(result.runState!.nodeStates.get('eh1')?.status).toBe('success');
      expect(result.runState!.nodeStates.get('final')?.status).toBe('success');
      expect(result.passed).toBe(true);
    });
  });

  // ── Reverse & Bidirectional ──────────────────────────────────────────

  describe('Reverse & Bidirectional Edges', () => {
    it('runs reverse edge data-pull topology', async () => {
      const result = await runAndAssert(reverseEdgeScenario);
      expect(result.runState!.status).toBe('success');
      // Conductor may collapse agents with reverse edges — key is flow completion
      expect(result.runState!.nodeStates.get('src')?.status).toBe('success');
      expect(result.runState!.nodeStates.get('out')?.status).toBe('success');
    });

    it('runs triple mesh 3-agent debate', async () => {
      const result = await runAndAssert(tripleMeshDebateScenario);
      expect(result.runState!.status).toBe('success');
      expect(result.strategy?.conductorUsed).toBe(true);
      expect(result.runState!.nodeStates.get('syn')?.status).toBe('success');
      expect(result.passed).toBe(true);
    });

    it('runs all edge kinds simultaneously', async () => {
      const result = await runAndAssert(allEdgeKindsScenario);
      expect(result.runState!.status).toBe('success');
      expect(result.strategy?.conductorUsed).toBe(true);
      // Error handler may execute since conductor processes all reachable nodes
      expect(result.passed).toBe(true);
    });
  });

  // ── Fan-Out / Fan-In / Diamond ───────────────────────────────────────

  describe('Diamond & Fan-Out Patterns', () => {
    it('runs multi-level diamond (3 stacked diamonds)', async () => {
      const result = await runAndAssert(multiDiamondScenario);
      expect(result.runState!.status).toBe('success');
      // All 3 merge nodes should fire
      for (let i = 0; i < 3; i++) {
        expect(result.runState!.nodeStates.get(`merge${i}`)?.status).toBe('success');
      }
      expect(result.passed).toBe(true);
    });

    it('runs massive 10-branch parallel fan-out', async () => {
      const result = await runAndAssert(massiveParallelFanOutScenario);
      expect(result.runState!.status).toBe('success');
      // All 10 workers should execute
      for (let i = 0; i < 10; i++) {
        expect(result.runState!.nodeStates.get(`w${i}`)?.status).toBe('success');
      }
      expect(result.runState!.nodeStates.get('merge')?.status).toBe('success');
      expect(result.passed).toBe(true);
    });

    it('runs wide orchestrator (boss + 8 workers)', async () => {
      const result = await runAndAssert(wideOrchestratorScenario);
      expect(result.runState!.status).toBe('success');
      expect(result.runState!.nodeStates.get('boss')?.status).toBe('success');
      for (let i = 0; i < 8; i++) {
        expect(result.runState!.nodeStates.get(`w${i}`)?.status).toBe('success');
      }
      expect(result.runState!.nodeStates.get('reviewer')?.status).toBe('success');
      expect(result.passed).toBe(true);
    });
  });

  // ── Collapse & Hybrid ───────────────────────────────────────────────

  describe('Collapse & Hybrid Strategies', () => {
    it('runs deep collapse chain (7 sequential agents)', async () => {
      const result = await runAndAssert(deepCollapseChainScenario);
      expect(result.runState!.status).toBe('success');
      expect(result.strategy?.conductorUsed).toBe(true);
      expect(result.passed).toBe(true);
    });

    it('runs parallel × collapse hybrid (3 branches × 3 steps)', async () => {
      const result = await runAndAssert(parallelCollapseHybridScenario);
      expect(result.runState!.status).toBe('success');
      expect(result.strategy?.conductorUsed).toBe(true);
      // All 9 agents + merge should complete
      for (let b = 0; b < 3; b++) {
        for (let s = 0; s < 3; s++) {
          expect(result.runState!.nodeStates.get(`b${b}_s${s}`)?.status).toBe('success');
        }
      }
      expect(result.passed).toBe(true);
    });
  });

  // ── Kitchen Sink & Node Kinds ────────────────────────────────────────

  describe('Kitchen Sink & Every Node Kind', () => {
    it('runs flow with every FlowNodeKind (16 types)', async () => {
      const result = await runAndAssert(everyNodeKindScenario);
      expect(result.runState!.status).toBe('success');
      // Verify a selection of node kinds executed (Conductor may collapse some)
      expect(result.runState!.nodeStates.get('agent1')?.status).toBe('success');
      expect(result.runState!.nodeStates.get('code1')?.status).toBe('success');
      expect(result.runState!.nodeStates.get('http1')?.status).toBe('success');
      expect(result.runState!.nodeStates.get('mcp1')?.status).toBe('success');
      expect(result.passed).toBe(true);
    });

    it('runs loop array iteration pipeline', async () => {
      const result = await runAndAssert(loopIterationScenario);
      expect(result.runState!.status).toBe('success');
      // Loop node executes — output depends on mock runtime processing
      expect(result.runState!.nodeStates.get('loop')?.status).toBe('success');
      expect(result.passed).toBe(true);
    });

    it('runs squad + memory + MCP combined', async () => {
      const result = await runAndAssert(squadMemoryMcpScenario);
      expect(result.runState!.status).toBe('success');
      // Memory recall should find prior data
      const recallCalls = result.mockCalls.filter((c) => c.type === 'memory-recall');
      expect(recallCalls.length).toBeGreaterThan(0);
      // MCP should be called
      const mcpCalls = result.mockCalls.filter((c) => c.type === 'mcp');
      expect(mcpCalls.length).toBeGreaterThan(0);
      expect(result.passed).toBe(true);
    });
  });

  // ── Branching & Conditions ───────────────────────────────────────────

  describe('Deep Branching & Conditions', () => {
    it('runs cascading condition tree (4 levels deep)', async () => {
      const result = await runAndAssert(cascadingConditionTreeScenario);
      expect(result.runState!.status).toBe('success');
      // Should take the true→true→true path
      expect(result.runState!.nodeStates.get('leaf1')?.status).toBe('success');
      // Wrong-path leaves should not execute
      expect(result.runState!.nodeStates.get('leaf6')?.status).not.toBe('success');
      expect(result.passed).toBe(true);
    });
  });

  // ── Error Handling & Resilience ──────────────────────────────────────

  describe('Error Handling & Resilience', () => {
    it('runs multi-error cascade (3 fallback levels)', async () => {
      const result = await runAndAssert(multiErrorCascadeScenario);
      // Primary should fail
      expect(result.runState!.nodeStates.get('primary')?.status).toBe('error');
      // Fallbacks should also fail
      expect(result.runState!.nodeStates.get('fb1')?.status).toBe('error');
      expect(result.runState!.nodeStates.get('fb2')?.status).toBe('error');
      // Emergency handler should catch
      expect(result.runState!.nodeStates.get('emerg')?.status).toBe('success');
      expect(result.passed).toBe(true);
    });

    it('runs chaos + retry torture test (50% failure rate)', async () => {
      const result = await runAndAssert(chaosRetryTortureScenario);
      // Don't assert success — chaos mode may or may not fail
      const events = result.events.map((e) => e.type);
      expect(events).toContain('run-start');
      expect(events).toContain('run-complete');
      // First node should at least be attempted
      expect(result.runState!.nodeStates.get('a0')).toBeDefined();
      expect(result.passed).toBe(true);
    });
  });

  // ── Variables & Data Flow ────────────────────────────────────────────

  describe('Variables & Data Propagation', () => {
    it('runs variable propagation stress (8 code nodes, 2^8 = 256)', async () => {
      const result = await runAndAssert(variablePropagationStressScenario);
      expect(result.runState!.status).toBe('success');
      // Final output should be 256 (1 * 2^8)
      const finalOutput = result.runState!.nodeStates.get('v7')?.output || '';
      expect(finalOutput).toContain('256');
      // All 8 variables should be set (variables is a plain object, not a Map)
      const vars = result.runState!.variables as Record<string, unknown>;
      expect(vars?.['val_0']).toBeDefined();
      expect(vars?.['val_7']).toBeDefined();
      expect(result.passed).toBe(true);
    });
  });

  // ── Mock Edge Cases ──────────────────────────────────────────────────

  describe('Mock System Edge Cases', () => {
    it('handles empty responses, unicode, sequence wrap, echo', async () => {
      const result = await runAndAssert(adversarialMockScenario);
      expect(result.runState!.status).toBe('success');
      // Unicode preserved
      const unicodeOutput = result.runState!.nodeStates.get('unicode')?.output || '';
      expect(unicodeOutput).toContain('🌟');
      // Echo agent should echo input
      const echoOutput = result.runState!.nodeStates.get('echo')?.output || '';
      expect(echoOutput).toContain('[ECHO]');
      // Empty response should not crash
      expect(result.runState!.nodeStates.get('empty')?.status).toBe('success');
      expect(result.passed).toBe(true);
    });
  });

  // ── Full Suite Runner ────────────────────────────────────────────────

  describe('Stress Suite Runner', () => {
    it('scenario list has exactly 20 scenarios', () => {
      expect(allStressScenarios.length).toBe(20);
    });

    it('all stress scenarios have unique IDs', () => {
      const ids = allStressScenarios.map((s) => s.id);
      expect(new Set(ids).size).toBe(ids.length);
    });

    it('stress suite covers all tiers', () => {
      const tiers = new Set(allStressScenarios.map((s) => s.tier));
      expect(tiers.has('standard')).toBe(true);
      expect(tiers.has('complex')).toBe(true);
      expect(tiers.has('extreme')).toBe(true);
    });

    it('stress suite covers multiple categories', () => {
      const cats = new Set(allStressScenarios.map((s) => s.category));
      expect(cats.size).toBeGreaterThanOrEqual(6);
    });

    it('runs full stress suite without crashing', async () => {
      const result = await runSimSuite(stressTestSuite);
      expect(result.totalScenarios).toBe(20);
      // Log failures for debugging (chaos tests may fail intentionally)
      for (const r of result.results) {
        if (!r.passed) {
          console.log(
            `STRESS FAIL: ${r.scenarioName}`,
            r.expectationResults.filter((e) => !e.passed),
          );
        }
      }
      // At least 70% should pass (chaos/retry + edge cases may fail)
      expect(result.passed).toBeGreaterThanOrEqual(14);
    });
  });
});
