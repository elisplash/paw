// ─── Canvas · Barrel Export ──────────────────────────────────────────────
// Re-exports all public API from atoms and molecules.

export {
  // Types
  type Canvas,
  type CanvasViewport,
  type CanvasNode,
  type CanvasEdge,

  // Constants
  NODE_KINDS,
  NODE_COLORS,
  EDGE_STYLES,
  DEFAULT_NODE_SIZE,
  MIN_ZOOM,
  MAX_ZOOM,
  ZOOM_STEP,
  GRID_SIZE,

  // Pure functions (atoms)
  snapToGrid,
  clampZoom,
  pointInNode,
  getNodeCenter,
  kindLabel,
  kindIcon,
} from './atoms';

export {
  // Canvas CRUD (molecules)
  listCanvases,
  createCanvas,
  updateCanvas,
  deleteCanvas,

  // Node CRUD (molecules)
  loadNodes,
  createNode,
  updateNode,
  deleteNode,

  // Edge CRUD (molecules)
  loadEdges,
  createEdge,
  deleteEdge,
} from './molecules';
