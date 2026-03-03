// ─────────────────────────────────────────────────────────────────────────────
// Flow Simulation Engine — Barrel Export
// "Holodeck Mode" for testing flow graphs in a fully mocked environment.
//
// Usage:
//   import { runSimulation, masterSimSuite } from './simulation';
//   const result = await runSimulation(masterSimSuite.scenarios[0]);
//   console.log(result.passed, result.expectationResults);
// ─────────────────────────────────────────────────────────────────────────────

// Types & pure atoms
export {
  type SimScenario,
  type SimCategory,
  type SimTier,
  type SimMockConfig,
  type MockAgentBehavior,
  type MockResponseStrategy,
  type MockResponseGenerator,
  type MockCallContext,
  type MockNodeBehavior,
  type MockHttpRule,
  type MockMcpResponse,
  type MockMemoryConfig,
  type MockMemoryEntry,
  type SimExpectation,
  type ExpectationType,
  type ExpectationCheck,
  type SimResult,
  type ExpectationResult,
  type MockCallLog,
  type SimSuite,
  type SimSuiteResult,
  // Factory helpers
  simNode,
  simEdge,
  simGraph,
  resetSimCounters,
  // Pure logic
  generateRealisticResponse,
  resolveMockResponse,
  evaluateExpectations,
} from './simulation-atoms';

// Runtime
export { runSimulation, runSimSuite } from './simulation-runtime';

// Built-in scenarios
export {
  basicLinearScenario,
  conditionBranchingScenario,
  conductorCollapseScenario,
  conductorParallelScenario,
  convergentMeshScenario,
  tesseractScenario,
  selfHealingScenario,
  integrationScenario,
  codeSandboxScenario,
  chaosTestingScenario,
  orchestratorScenario,
  mixedConductorScenario,
  masterSimSuite,
  getScenarioById,
  getScenariosByCategory,
  getScenariosByTier,
  getScenariosByTags,
} from './simulation-scenarios';

// Stress-test scenarios
export {
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
