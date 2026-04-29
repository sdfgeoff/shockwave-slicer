const state = {
  image: null,
  sourceCanvas: document.createElement("canvas"),
  sourceContext: null,
  imageData: null,
  processedCanvas: document.createElement("canvas"),
  processedContext: null,
  viewportScale: 1,
  minScale: 0.1,
  maxScale: 64,
  offsetX: 0,
  offsetY: 0,
  isPanning: false,
  panStartX: 0,
  panStartY: 0,
  columns: 4,
  rows: 4,
  showGrid: true,
  displayMode: "rgba",
  channel: "r",
  lowThreshold: 0,
  highThreshold: 255,
  hoveredCell: null,
  hoveredValue: null,
};

state.sourceContext = state.sourceCanvas.getContext("2d", { willReadFrequently: true });
state.processedContext = state.processedCanvas.getContext("2d", { willReadFrequently: true });

const viewport = document.getElementById("viewport");
const canvas = document.getElementById("viewerCanvas");
const context = canvas.getContext("2d");
const dropHint = document.getElementById("dropHint");

const imageInput = document.getElementById("imageInput");
const columnsInput = document.getElementById("columnsInput");
const rowsInput = document.getElementById("rowsInput");
const showGridInput = document.getElementById("showGridInput");
const displayModeInput = document.getElementById("displayModeInput");
const channelInput = document.getElementById("channelInput");
const lowThresholdInput = document.getElementById("lowThresholdInput");
const highThresholdInput = document.getElementById("highThresholdInput");
const lowThresholdValue = document.getElementById("lowThresholdValue");
const highThresholdValue = document.getElementById("highThresholdValue");
const fitButton = document.getElementById("fitButton");
const resetButton = document.getElementById("resetButton");

const imageSizeLabel = document.getElementById("imageSizeLabel");
const cellSizeLabel = document.getElementById("cellSizeLabel");
const hoveredCellLabel = document.getElementById("hoveredCellLabel");
const hoveredValueLabel = document.getElementById("hoveredValueLabel");

function resizeCanvasToViewport() {
  const rect = viewport.getBoundingClientRect();
  const pixelRatio = window.devicePixelRatio || 1;
  canvas.width = Math.floor(rect.width * pixelRatio);
  canvas.height = Math.floor(rect.height * pixelRatio);
  canvas.style.width = `${rect.width}px`;
  canvas.style.height = `${rect.height}px`;
  context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
  render();
}

function getChannelIndex(channel) {
  if (channel === "r") return 0;
  if (channel === "g") return 1;
  if (channel === "b") return 2;
  return 3;
}

function setThresholdLabels() {
  lowThresholdValue.value = `${state.lowThreshold}`;
  highThresholdValue.value = `${state.highThreshold}`;
}

function clampThresholds() {
  if (state.lowThreshold > state.highThreshold) {
    state.highThreshold = state.lowThreshold;
    highThresholdInput.value = `${state.highThreshold}`;
  }
  if (state.highThreshold < state.lowThreshold) {
    state.lowThreshold = state.highThreshold;
    lowThresholdInput.value = `${state.lowThreshold}`;
  }
  setThresholdLabels();
}

function updateStats() {
  if (!state.image) {
    imageSizeLabel.textContent = "No image loaded";
    cellSizeLabel.textContent = "-";
    hoveredCellLabel.textContent = "-";
    hoveredValueLabel.textContent = "-";
    return;
  }

  imageSizeLabel.textContent = `${state.image.width} x ${state.image.height}`;
  const cellWidth = state.image.width / state.columns;
  const cellHeight = state.image.height / state.rows;
  cellSizeLabel.textContent = `${cellWidth.toFixed(2)} x ${cellHeight.toFixed(2)}`;

  if (state.hoveredCell) {
    hoveredCellLabel.textContent = `${state.hoveredCell.index} (${state.hoveredCell.column}, ${state.hoveredCell.row})`;
  } else {
    hoveredCellLabel.textContent = "-";
  }

  if (state.hoveredValue) {
    hoveredValueLabel.textContent = `${state.hoveredValue.channel.toUpperCase()}: ${state.hoveredValue.value}`;
  } else {
    hoveredValueLabel.textContent = "-";
  }
}

function fitImageToViewport() {
  if (!state.image) {
    return;
  }

  const rect = viewport.getBoundingClientRect();
  const scaleX = rect.width / state.image.width;
  const scaleY = rect.height / state.image.height;
  state.viewportScale = Math.min(scaleX, scaleY) * 0.92;
  state.offsetX = (rect.width - state.image.width * state.viewportScale) / 2;
  state.offsetY = (rect.height - state.image.height * state.viewportScale) / 2;
  render();
}

function resetView() {
  if (!state.image) {
    return;
  }
  fitImageToViewport();
}

function processImage() {
  if (!state.imageData || !state.image) {
    return;
  }

  const source = state.imageData.data;
  const width = state.image.width;
  const height = state.image.height;
  const output = state.processedContext.createImageData(width, height);
  const target = output.data;
  const channelIndex = getChannelIndex(state.channel);

  for (let i = 0; i < source.length; i += 4) {
    const channelValue = source[i + channelIndex];

    if (state.displayMode === "rgba") {
      target[i] = source[i];
      target[i + 1] = source[i + 1];
      target[i + 2] = source[i + 2];
      target[i + 3] = 255;
      continue;
    }

    if (state.displayMode === "single") {
      target[i] = channelValue;
      target[i + 1] = channelValue;
      target[i + 2] = channelValue;
      target[i + 3] = 255;
      continue;
    }

    const withinRange = channelValue >= state.lowThreshold && channelValue <= state.highThreshold;
    const maskValue = withinRange ? 255 : 0;
    target[i] = maskValue;
    target[i + 1] = withinRange ? 220 : 18;
    target[i + 2] = withinRange ? 170 : 18;
    target[i + 3] = 255;
  }

  state.processedCanvas.width = width;
  state.processedCanvas.height = height;
  state.processedContext.putImageData(output, 0, 0);
}

function getImageCoordinates(event) {
  if (!state.image) {
    return null;
  }

  const rect = canvas.getBoundingClientRect();
  const x = (event.clientX - rect.left - state.offsetX) / state.viewportScale;
  const y = (event.clientY - rect.top - state.offsetY) / state.viewportScale;

  if (x < 0 || y < 0 || x >= state.image.width || y >= state.image.height) {
    return null;
  }

  return {
    x,
    y,
    pixelX: Math.floor(x),
    pixelY: Math.floor(y),
  };
}

function updateHoveredState(event) {
  const coords = getImageCoordinates(event);
  if (!coords || !state.imageData) {
    state.hoveredCell = null;
    state.hoveredValue = null;
    updateStats();
    render();
    return;
  }

  const cellWidth = state.image.width / state.columns;
  const cellHeight = state.image.height / state.rows;
  const column = Math.min(state.columns - 1, Math.floor(coords.x / cellWidth));
  const row = Math.min(state.rows - 1, Math.floor(coords.y / cellHeight));
  const index = row * state.columns + column;

  const pixelOffset = (coords.pixelY * state.image.width + coords.pixelX) * 4;
  const channelIndex = getChannelIndex(state.channel);
  const value = state.imageData.data[pixelOffset + channelIndex];

  state.hoveredCell = { column, row, index };
  state.hoveredValue = { channel: state.channel, value };
  updateStats();
  render();
}

function drawGrid() {
  if (!state.image || !state.showGrid) {
    return;
  }

  context.save();
  context.translate(state.offsetX, state.offsetY);
  context.scale(state.viewportScale, state.viewportScale);
  context.strokeStyle = "rgba(255, 255, 255, 0.2)";
  context.lineWidth = 1 / state.viewportScale;

  const cellWidth = state.image.width / state.columns;
  const cellHeight = state.image.height / state.rows;

  for (let column = 0; column <= state.columns; column += 1) {
    const x = column * cellWidth;
    context.beginPath();
    context.moveTo(x, 0);
    context.lineTo(x, state.image.height);
    context.stroke();
  }

  for (let row = 0; row <= state.rows; row += 1) {
    const y = row * cellHeight;
    context.beginPath();
    context.moveTo(0, y);
    context.lineTo(state.image.width, y);
    context.stroke();
  }

  if (state.hoveredCell) {
    context.strokeStyle = "rgba(103, 213, 181, 0.95)";
    context.lineWidth = 2 / state.viewportScale;
    context.strokeRect(
      state.hoveredCell.column * cellWidth,
      state.hoveredCell.row * cellHeight,
      cellWidth,
      cellHeight,
    );
  }

  context.restore();
}

function render() {
  const rect = viewport.getBoundingClientRect();
  context.clearRect(0, 0, rect.width, rect.height);

  if (!state.image) {
    return;
  }

  context.save();
  context.translate(state.offsetX, state.offsetY);
  context.scale(state.viewportScale, state.viewportScale);
  context.imageSmoothingEnabled = false;
  context.drawImage(state.processedCanvas, 0, 0);
  context.restore();

  drawGrid();
}

function loadImageFromUrl(url) {
  const image = new Image();
  image.onload = () => {
    state.image = image;
    state.sourceCanvas.width = image.width;
    state.sourceCanvas.height = image.height;
    state.sourceContext.clearRect(0, 0, image.width, image.height);
    state.sourceContext.drawImage(image, 0, 0);
    state.imageData = state.sourceContext.getImageData(0, 0, image.width, image.height);
    state.hoveredCell = null;
    state.hoveredValue = null;
    processImage();
    fitImageToViewport();
    updateStats();
    dropHint.classList.add("hidden");
  };
  image.src = url;
}

function handleFile(file) {
  if (!file || !file.type.startsWith("image/")) {
    return;
  }

  const reader = new FileReader();
  reader.onload = () => {
    loadImageFromUrl(reader.result);
  };
  reader.readAsDataURL(file);
}

imageInput.addEventListener("change", (event) => {
  handleFile(event.target.files?.[0] ?? null);
});

columnsInput.addEventListener("input", () => {
  state.columns = Math.max(1, Number.parseInt(columnsInput.value, 10) || 1);
  updateStats();
  render();
});

rowsInput.addEventListener("input", () => {
  state.rows = Math.max(1, Number.parseInt(rowsInput.value, 10) || 1);
  updateStats();
  render();
});

showGridInput.addEventListener("change", () => {
  state.showGrid = showGridInput.checked;
  render();
});

displayModeInput.addEventListener("change", () => {
  state.displayMode = displayModeInput.value;
  processImage();
  render();
});

channelInput.addEventListener("change", () => {
  state.channel = channelInput.value;
  processImage();
  updateStats();
  render();
});

lowThresholdInput.addEventListener("input", () => {
  state.lowThreshold = Number.parseInt(lowThresholdInput.value, 10);
  clampThresholds();
  processImage();
  render();
});

highThresholdInput.addEventListener("input", () => {
  state.highThreshold = Number.parseInt(highThresholdInput.value, 10);
  clampThresholds();
  processImage();
  render();
});

fitButton.addEventListener("click", fitImageToViewport);
resetButton.addEventListener("click", resetView);

canvas.addEventListener("mousedown", (event) => {
  state.isPanning = true;
  state.panStartX = event.clientX - state.offsetX;
  state.panStartY = event.clientY - state.offsetY;
  canvas.classList.add("is-panning");
});

window.addEventListener("mousemove", (event) => {
  if (state.isPanning) {
    state.offsetX = event.clientX - state.panStartX;
    state.offsetY = event.clientY - state.panStartY;
    render();
  } else {
    updateHoveredState(event);
  }
});

window.addEventListener("mouseup", () => {
  state.isPanning = false;
  canvas.classList.remove("is-panning");
});

canvas.addEventListener("mouseleave", () => {
  if (!state.isPanning) {
    state.hoveredCell = null;
    state.hoveredValue = null;
    updateStats();
    render();
  }
});

canvas.addEventListener(
  "wheel",
  (event) => {
    if (!state.image) {
      return;
    }

    event.preventDefault();

    const rect = canvas.getBoundingClientRect();
    const mouseX = event.clientX - rect.left;
    const mouseY = event.clientY - rect.top;
    const zoomFactor = event.deltaY < 0 ? 1.1 : 0.9;
    const nextScale = Math.min(state.maxScale, Math.max(state.minScale, state.viewportScale * zoomFactor));

    const originX = (mouseX - state.offsetX) / state.viewportScale;
    const originY = (mouseY - state.offsetY) / state.viewportScale;

    state.viewportScale = nextScale;
    state.offsetX = mouseX - originX * state.viewportScale;
    state.offsetY = mouseY - originY * state.viewportScale;
    render();
  },
  { passive: false },
);

viewport.addEventListener("dragover", (event) => {
  event.preventDefault();
  viewport.classList.add("drag-over");
});

viewport.addEventListener("dragleave", () => {
  viewport.classList.remove("drag-over");
});

viewport.addEventListener("drop", (event) => {
  event.preventDefault();
  viewport.classList.remove("drag-over");
  handleFile(event.dataTransfer?.files?.[0] ?? null);
});

window.addEventListener("resize", resizeCanvasToViewport);

setThresholdLabels();
resizeCanvasToViewport();
updateStats();
