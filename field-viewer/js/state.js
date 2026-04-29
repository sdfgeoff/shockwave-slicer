export const state = {
  image: null,
  imageData: null,
  processedCanvas: document.createElement("canvas"),
  processedContext: null,
  viewMode: "2d",
  viewportScale: 1,
  minScale: 0.1,
  maxScale: 64,
  offsetX: 0,
  offsetY: 0,
  isDragging: false,
  dragStartX: 0,
  dragStartY: 0,
  columns: 4,
  rows: 4,
  showGrid: true,
  displayMode: "rgba",
  channel: "r",
  lowThreshold: 0,
  highThreshold: 255,
  hoveredCell: null,
  hoveredValue: null,
  orbitYaw: -0.75,
  orbitPitch: 0.55,
  cameraDistance: 2.8,
};

state.processedContext = state.processedCanvas.getContext("2d", { willReadFrequently: true });

export function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

export function getChannelIndex(channel) {
  if (channel === "r") return 0;
  if (channel === "g") return 1;
  if (channel === "b") return 2;
  return 3;
}

export function getCellDimensions() {
  if (!state.image) {
    return { width: 0, height: 0 };
  }

  return {
    width: Math.floor(state.image.width / state.columns),
    height: Math.floor(state.image.height / state.rows),
  };
}

export function getVolumeDimensions() {
  const cell = getCellDimensions();
  return {
    width: cell.width,
    height: cell.height,
    depth: state.columns * state.rows,
  };
}

export function clampThresholds() {
  if (state.lowThreshold > state.highThreshold) {
    state.highThreshold = state.lowThreshold;
  }
  if (state.highThreshold < state.lowThreshold) {
    state.lowThreshold = state.highThreshold;
  }
}
