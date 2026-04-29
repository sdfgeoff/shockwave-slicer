export const state = {
  image: null,
  imageData: null,
  processedCanvas: document.createElement("canvas"),
  processedContext: null,
  viewMode: "2d",
  viewportScale: 1,
  minScale: 0.1,
  maxScale: 512,
  offsetX: 0,
  offsetY: 0,
  isDragging: false,
  dragStartX: 0,
  dragStartY: 0,
  columns: 4,
  rows: 4,
  depth: 16,
  columnsFromUrl: false,
  rowsFromUrl: false,
  depthFromUrl: false,
  showGrid: true,
  dataMode: "field-occupancy",
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
    depth: Math.min(state.depth, state.columns * state.rows),
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

export function suggestGridDimensions(imageWidth, imageHeight) {
  const widthDivisors = getDivisors(imageWidth);
  const heightDivisors = getDivisors(imageHeight);
  let best = {
    columns: 1,
    rows: 1,
    score: Number.POSITIVE_INFINITY,
  };

  for (const columns of widthDivisors) {
    const cellWidth = imageWidth / columns;

    for (const rows of heightDivisors) {
      const cellHeight = imageHeight / rows;
      const depth = columns * rows;
      const axisAverage = (cellWidth + cellHeight + depth) / 3;
      const cellAspectPenalty = Math.abs(cellWidth - cellHeight) / Math.max(axisAverage, 1);
      const cubicPenalty = (
        Math.abs(cellWidth - axisAverage) +
        Math.abs(cellHeight - axisAverage) +
        Math.abs(depth - axisAverage)
      ) / Math.max(axisAverage, 1);
      const atlasShapePenalty = Math.abs(columns - rows) / Math.max(Math.sqrt(depth), 1);
      const score = cubicPenalty * 1.8 + cellAspectPenalty * 1.4 + atlasShapePenalty * 0.35;

      if (score < best.score) {
        best = { columns, rows, score };
      }
    }
  }

  return {
    columns: best.columns,
    rows: best.rows,
  };
}

function getDivisors(value) {
  const divisors = [];
  const pairedDivisors = [];

  for (let candidate = 1; candidate * candidate <= value; candidate += 1) {
    if (value % candidate !== 0) {
      continue;
    }

    divisors.push(candidate);
    const pair = value / candidate;
    if (pair !== candidate) {
      pairedDivisors.push(pair);
    }
  }

  return divisors.concat(pairedDivisors.reverse());
}
