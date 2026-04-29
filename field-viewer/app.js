const state = {
  image: null,
  sourceCanvas: document.createElement("canvas"),
  sourceContext: null,
  imageData: null,
  processedCanvas: document.createElement("canvas"),
  processedContext: null,
  volumeCanvas: document.createElement("canvas"),
  volumeContext: null,
  volumeData: null,
  volumeWidth: 0,
  volumeHeight: 0,
  volumeDepth: 0,
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
  renderQueued: false,
};

state.sourceContext = state.sourceCanvas.getContext("2d", { willReadFrequently: true });
state.processedContext = state.processedCanvas.getContext("2d", { willReadFrequently: true });
state.volumeContext = state.volumeCanvas.getContext("2d", { willReadFrequently: true });

const viewport = document.getElementById("viewport");
const canvas = document.getElementById("viewerCanvas");
const context = canvas.getContext("2d");
const dropHint = document.getElementById("dropHint");

const imageInput = document.getElementById("imageInput");
const columnsInput = document.getElementById("columnsInput");
const rowsInput = document.getElementById("rowsInput");
const showGridInput = document.getElementById("showGridInput");
const viewModeInput = document.getElementById("viewModeInput");
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

function requestRender() {
  if (state.renderQueued) {
    return;
  }
  state.renderQueued = true;
  window.requestAnimationFrame(() => {
    state.renderQueued = false;
    render();
  });
}

function resizeCanvasToViewport() {
  const rect = viewport.getBoundingClientRect();
  const pixelRatio = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.floor(rect.width * pixelRatio));
  canvas.height = Math.max(1, Math.floor(rect.height * pixelRatio));
  canvas.style.width = `${rect.width}px`;
  canvas.style.height = `${rect.height}px`;
  context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
  requestRender();
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
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

function getCellDimensions() {
  if (!state.image) {
    return { width: 0, height: 0 };
  }

  return {
    width: Math.floor(state.image.width / state.columns),
    height: Math.floor(state.image.height / state.rows),
  };
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
  const cell = getCellDimensions();
  cellSizeLabel.textContent = `${cell.width} x ${cell.height}`;

  if (state.viewMode === "3d") {
    hoveredCellLabel.textContent = `${state.volumeWidth} x ${state.volumeHeight} x ${state.volumeDepth}`;
    hoveredValueLabel.textContent = `${state.channel.toUpperCase()} ${state.lowThreshold}-${state.highThreshold}`;
    return;
  }

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
  requestRender();
}

function reset3DView() {
  state.orbitYaw = -0.75;
  state.orbitPitch = 0.55;
  state.cameraDistance = 2.8;
  requestRender();
}

function resetView() {
  if (!state.image) {
    return;
  }

  if (state.viewMode === "2d") {
    fitImageToViewport();
    return;
  }

  reset3DView();
}

function processImage() {
  if (!state.imageData || !state.image) {
    return;
  }

  const source = state.imageData.data;
  const width = state.image.width;
  const height = state.image.height;
  state.processedCanvas.width = width;
  state.processedCanvas.height = height;
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
    target[i] = withinRange ? 255 : 18;
    target[i + 1] = withinRange ? 220 : 18;
    target[i + 2] = withinRange ? 170 : 18;
    target[i + 3] = 255;
  }

  state.processedContext.putImageData(output, 0, 0);
}

function rebuildVolume() {
  if (!state.imageData || !state.image) {
    state.volumeData = null;
    state.volumeWidth = 0;
    state.volumeHeight = 0;
    state.volumeDepth = 0;
    return;
  }

  const cell = getCellDimensions();
  const depth = state.columns * state.rows;
  const source = state.imageData.data;
  const volume = new Uint8ClampedArray(cell.width * cell.height * depth * 4);

  for (let slice = 0; slice < depth; slice += 1) {
    const cellColumn = slice % state.columns;
    const cellRow = Math.floor(slice / state.columns);

    for (let y = 0; y < cell.height; y += 1) {
      for (let x = 0; x < cell.width; x += 1) {
        const atlasX = cellColumn * cell.width + x;
        const atlasY = cellRow * cell.height + y;
        const sourceOffset = (atlasY * state.image.width + atlasX) * 4;
        const volumeOffset = (((slice * cell.height + y) * cell.width) + x) * 4;

        volume[volumeOffset] = source[sourceOffset];
        volume[volumeOffset + 1] = source[sourceOffset + 1];
        volume[volumeOffset + 2] = source[sourceOffset + 2];
        volume[volumeOffset + 3] = source[sourceOffset + 3];
      }
    }
  }

  state.volumeData = volume;
  state.volumeWidth = cell.width;
  state.volumeHeight = cell.height;
  state.volumeDepth = depth;
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
  if (state.viewMode !== "2d") {
    state.hoveredCell = null;
    state.hoveredValue = null;
    updateStats();
    return;
  }

  const coords = getImageCoordinates(event);
  if (!coords || !state.imageData) {
    state.hoveredCell = null;
    state.hoveredValue = null;
    updateStats();
    requestRender();
    return;
  }

  const cell = getCellDimensions();
  const column = Math.min(state.columns - 1, Math.floor(coords.x / cell.width));
  const row = Math.min(state.rows - 1, Math.floor(coords.y / cell.height));
  const index = row * state.columns + column;
  const pixelOffset = (coords.pixelY * state.image.width + coords.pixelX) * 4;
  const channelIndex = getChannelIndex(state.channel);

  state.hoveredCell = { column, row, index };
  state.hoveredValue = { channel: state.channel, value: state.imageData.data[pixelOffset + channelIndex] };
  updateStats();
  requestRender();
}

function drawGrid() {
  if (!state.image || !state.showGrid || state.viewMode !== "2d") {
    return;
  }

  const cell = getCellDimensions();
  context.save();
  context.translate(state.offsetX, state.offsetY);
  context.scale(state.viewportScale, state.viewportScale);
  context.strokeStyle = "rgba(255, 255, 255, 0.2)";
  context.lineWidth = 1 / state.viewportScale;

  for (let column = 0; column <= state.columns; column += 1) {
    const x = column * cell.width;
    context.beginPath();
    context.moveTo(x, 0);
    context.lineTo(x, state.image.height);
    context.stroke();
  }

  for (let row = 0; row <= state.rows; row += 1) {
    const y = row * cell.height;
    context.beginPath();
    context.moveTo(0, y);
    context.lineTo(state.image.width, y);
    context.stroke();
  }

  if (state.hoveredCell) {
    context.strokeStyle = "rgba(103, 213, 181, 0.95)";
    context.lineWidth = 2 / state.viewportScale;
    context.strokeRect(
      state.hoveredCell.column * cell.width,
      state.hoveredCell.row * cell.height,
      cell.width,
      cell.height,
    );
  }

  context.restore();
}

function rotateX(vector, angle) {
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  return {
    x: vector.x,
    y: vector.y * cos - vector.z * sin,
    z: vector.y * sin + vector.z * cos,
  };
}

function rotateY(vector, angle) {
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  return {
    x: vector.x * cos + vector.z * sin,
    y: vector.y,
    z: -vector.x * sin + vector.z * cos,
  };
}

function inverseRotate(vector) {
  return rotateY(rotateX(vector, -state.orbitPitch), -state.orbitYaw);
}

function normalize(vector) {
  const length = Math.hypot(vector.x, vector.y, vector.z) || 1;
  return {
    x: vector.x / length,
    y: vector.y / length,
    z: vector.z / length,
  };
}

function intersectUnitCube(origin, direction) {
  let tMin = -Infinity;
  let tMax = Infinity;

  for (const axis of ["x", "y", "z"]) {
    const originValue = origin[axis];
    const directionValue = direction[axis];

    if (Math.abs(directionValue) < 1e-6) {
      if (originValue < -1 || originValue > 1) {
        return null;
      }
      continue;
    }

    const t1 = (-1 - originValue) / directionValue;
    const t2 = (1 - originValue) / directionValue;
    const near = Math.min(t1, t2);
    const far = Math.max(t1, t2);
    tMin = Math.max(tMin, near);
    tMax = Math.min(tMax, far);

    if (tMin > tMax) {
      return null;
    }
  }

  if (tMax < 0) {
    return null;
  }

  return {
    start: Math.max(0, tMin),
    end: tMax,
  };
}

function sampleVolume(x, y, z, channelIndex) {
  const ix = clamp(Math.floor(((x + 1) * 0.5) * state.volumeWidth), 0, state.volumeWidth - 1);
  const iy = clamp(Math.floor(((y + 1) * 0.5) * state.volumeHeight), 0, state.volumeHeight - 1);
  const iz = clamp(Math.floor(((z + 1) * 0.5) * state.volumeDepth), 0, state.volumeDepth - 1);
  const offset = (((iz * state.volumeHeight + iy) * state.volumeWidth) + ix) * 4;
  return state.volumeData[offset + channelIndex];
}

function renderVolume() {
  if (!state.volumeData || !state.volumeWidth || !state.volumeHeight || !state.volumeDepth) {
    return;
  }

  const rect = viewport.getBoundingClientRect();
  const aspect = rect.width / Math.max(rect.height, 1);
  const maxDimension = 220;
  const scale = Math.min(1, maxDimension / Math.max(rect.width, rect.height));
  const renderWidth = Math.max(96, Math.floor(rect.width * scale));
  const renderHeight = Math.max(96, Math.floor(rect.height * scale));
  state.volumeCanvas.width = renderWidth;
  state.volumeCanvas.height = renderHeight;

  const imageData = state.volumeContext.createImageData(renderWidth, renderHeight);
  const target = imageData.data;
  const backgroundTop = [20, 32, 40];
  const backgroundBottom = [6, 10, 14];
  const baseColorByChannel = {
    r: [255, 120, 120],
    g: [112, 255, 170],
    b: [120, 170, 255],
    a: [255, 222, 132],
  };
  const channelColor = baseColorByChannel[state.channel];
  const channelIndex = getChannelIndex(state.channel);
  const stepCount = Math.max(48, Math.min(180, Math.ceil(Math.max(
    state.volumeWidth,
    state.volumeHeight,
    state.volumeDepth,
  ) * 1.4)));
  const fov = 1.15;

  for (let py = 0; py < renderHeight; py += 1) {
    const v = py / Math.max(renderHeight - 1, 1);
    const background = [
      Math.round(backgroundTop[0] * (1 - v) + backgroundBottom[0] * v),
      Math.round(backgroundTop[1] * (1 - v) + backgroundBottom[1] * v),
      Math.round(backgroundTop[2] * (1 - v) + backgroundBottom[2] * v),
    ];

    for (let px = 0; px < renderWidth; px += 1) {
      const screenX = (((px + 0.5) / renderWidth) * 2 - 1) * aspect * fov;
      const screenY = (1 - ((py + 0.5) / renderHeight) * 2) * fov;
      const origin = inverseRotate({ x: 0, y: 0, z: state.cameraDistance });
      const direction = normalize(inverseRotate({ x: screenX, y: screenY, z: -1 }));
      const hit = intersectUnitCube(origin, direction);
      const outputOffset = (py * renderWidth + px) * 4;

      if (!hit) {
        target[outputOffset] = background[0];
        target[outputOffset + 1] = background[1];
        target[outputOffset + 2] = background[2];
        target[outputOffset + 3] = 255;
        continue;
      }

      const totalDistance = hit.end - hit.start;
      const dt = totalDistance / stepCount;
      let t = hit.start;
      let alpha = 0;
      let red = background[0] * 0.2;
      let green = background[1] * 0.2;
      let blue = background[2] * 0.2;

      for (let step = 0; step < stepCount; step += 1) {
        const x = origin.x + direction.x * t;
        const y = origin.y + direction.y * t;
        const z = origin.z + direction.z * t;
        const value = sampleVolume(x, y, z, channelIndex);

        if (value >= state.lowThreshold && value <= state.highThreshold) {
          const normalizedValue = value / 255;
          const localAlpha = 0.08 + normalizedValue * 0.16;
          const remaining = 1 - alpha;

          red += remaining * localAlpha * channelColor[0] * (0.45 + normalizedValue * 0.55);
          green += remaining * localAlpha * channelColor[1] * (0.45 + normalizedValue * 0.55);
          blue += remaining * localAlpha * channelColor[2] * (0.45 + normalizedValue * 0.55);
          alpha += remaining * localAlpha;

          if (alpha >= 0.98) {
            break;
          }
        }

        t += dt;
      }

      target[outputOffset] = clamp(Math.round(red + background[0] * (1 - alpha)), 0, 255);
      target[outputOffset + 1] = clamp(Math.round(green + background[1] * (1 - alpha)), 0, 255);
      target[outputOffset + 2] = clamp(Math.round(blue + background[2] * (1 - alpha)), 0, 255);
      target[outputOffset + 3] = 255;
    }
  }

  state.volumeContext.putImageData(imageData, 0, 0);
  context.imageSmoothingEnabled = false;
  context.drawImage(state.volumeCanvas, 0, 0, rect.width, rect.height);

  context.save();
  context.fillStyle = "rgba(255, 255, 255, 0.65)";
  context.font = '12px "IBM Plex Sans", "Segoe UI", sans-serif';
  context.fillText("Volume view", 18, 24);
  context.fillText(
    `${state.volumeWidth} x ${state.volumeHeight} x ${state.volumeDepth} voxels`,
    18,
    42,
  );
  context.restore();
}

function renderAtlas() {
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

function render() {
  const rect = viewport.getBoundingClientRect();
  context.clearRect(0, 0, rect.width, rect.height);

  if (!state.image) {
    return;
  }

  if (state.viewMode === "3d") {
    renderVolume();
  } else {
    renderAtlas();
  }
}

function refreshDerivedData() {
  processImage();
  rebuildVolume();
  updateStats();
  requestRender();
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
    refreshDerivedData();
    fitImageToViewport();
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
  refreshDerivedData();
});

rowsInput.addEventListener("input", () => {
  state.rows = Math.max(1, Number.parseInt(rowsInput.value, 10) || 1);
  refreshDerivedData();
});

showGridInput.addEventListener("change", () => {
  state.showGrid = showGridInput.checked;
  requestRender();
});

viewModeInput.addEventListener("change", () => {
  state.viewMode = viewModeInput.value;
  state.hoveredCell = null;
  state.hoveredValue = null;
  updateStats();
  requestRender();
});

displayModeInput.addEventListener("change", () => {
  state.displayMode = displayModeInput.value;
  processImage();
  requestRender();
});

channelInput.addEventListener("change", () => {
  state.channel = channelInput.value;
  processImage();
  updateStats();
  requestRender();
});

lowThresholdInput.addEventListener("input", () => {
  state.lowThreshold = Number.parseInt(lowThresholdInput.value, 10);
  clampThresholds();
  updateStats();
  requestRender();
});

highThresholdInput.addEventListener("input", () => {
  state.highThreshold = Number.parseInt(highThresholdInput.value, 10);
  clampThresholds();
  updateStats();
  requestRender();
});

fitButton.addEventListener("click", () => {
  if (state.viewMode === "2d") {
    fitImageToViewport();
  } else {
    reset3DView();
  }
});

resetButton.addEventListener("click", resetView);

canvas.addEventListener("mousedown", (event) => {
  state.isDragging = true;
  state.dragStartX = event.clientX;
  state.dragStartY = event.clientY;
  if (state.viewMode === "2d") {
    state.dragStartX = event.clientX - state.offsetX;
    state.dragStartY = event.clientY - state.offsetY;
  }
  canvas.classList.add("is-panning");
});

window.addEventListener("mousemove", (event) => {
  if (state.isDragging) {
    if (state.viewMode === "2d") {
      state.offsetX = event.clientX - state.dragStartX;
      state.offsetY = event.clientY - state.dragStartY;
    } else {
      const deltaX = event.clientX - state.dragStartX;
      const deltaY = event.clientY - state.dragStartY;
      state.dragStartX = event.clientX;
      state.dragStartY = event.clientY;
      state.orbitYaw += deltaX * 0.01;
      state.orbitPitch = clamp(state.orbitPitch + deltaY * 0.01, -1.45, 1.45);
    }
    requestRender();
    return;
  }

  updateHoveredState(event);
});

window.addEventListener("mouseup", () => {
  state.isDragging = false;
  canvas.classList.remove("is-panning");
});

canvas.addEventListener("mouseleave", () => {
  if (!state.isDragging && state.viewMode === "2d") {
    state.hoveredCell = null;
    state.hoveredValue = null;
    updateStats();
    requestRender();
  }
});

canvas.addEventListener(
  "wheel",
  (event) => {
    if (!state.image) {
      return;
    }

    event.preventDefault();

    if (state.viewMode === "2d") {
      const rect = canvas.getBoundingClientRect();
      const mouseX = event.clientX - rect.left;
      const mouseY = event.clientY - rect.top;
      const zoomFactor = event.deltaY < 0 ? 1.1 : 0.9;
      const nextScale = clamp(state.viewportScale * zoomFactor, state.minScale, state.maxScale);
      const originX = (mouseX - state.offsetX) / state.viewportScale;
      const originY = (mouseY - state.offsetY) / state.viewportScale;

      state.viewportScale = nextScale;
      state.offsetX = mouseX - originX * state.viewportScale;
      state.offsetY = mouseY - originY * state.viewportScale;
    } else {
      const zoomFactor = event.deltaY < 0 ? 0.9 : 1.1;
      state.cameraDistance = clamp(state.cameraDistance * zoomFactor, 1.4, 6.0);
    }

    requestRender();
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
