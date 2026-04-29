import { AtlasView } from "./atlas-view.js";
import { handleFile, processImage } from "./image-model.js";
import {
  clamp,
  clampThresholds,
  getCellDimensions,
  getVolumeDimensions,
  state,
  suggestGridDimensions,
} from "./state.js";
import { VolumeView } from "./volume-view.js";

const viewport = document.getElementById("viewport");
const atlasCanvas = document.getElementById("atlasCanvas");
const volumeCanvas = document.getElementById("volumeCanvas");
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

const atlasView = new AtlasView(atlasCanvas, viewport);
const volumeView = new VolumeView(volumeCanvas, viewport);

let renderQueued = false;

function requestRender() {
  if (renderQueued) {
    return;
  }
  renderQueued = true;
  window.requestAnimationFrame(() => {
    renderQueued = false;
    render();
  });
}

function setThresholdLabels() {
  lowThresholdValue.value = `${state.lowThreshold}`;
  highThresholdValue.value = `${state.highThreshold}`;
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
    const volume = getVolumeDimensions();
    hoveredCellLabel.textContent = `${volume.width} x ${volume.height} x ${volume.depth}`;
    hoveredValueLabel.textContent = `${state.channel.toUpperCase()} ${state.lowThreshold}-${state.highThreshold}`;
    return;
  }

  hoveredCellLabel.textContent = state.hoveredCell
    ? `${state.hoveredCell.index} (${state.hoveredCell.column}, ${state.hoveredCell.row})`
    : "-";
  hoveredValueLabel.textContent = state.hoveredValue
    ? `${state.hoveredValue.channel.toUpperCase()}: ${state.hoveredValue.value}`
    : "-";
}

function resizeViews() {
  atlasView.resize();
  volumeView.resize();
  requestRender();
}

function syncActiveCanvas() {
  const is2d = state.viewMode === "2d";
  atlasCanvas.classList.toggle("hidden", !is2d);
  volumeCanvas.classList.toggle("hidden", is2d);
}

function refreshImageDerivedState() {
  processImage();
  volumeView.updateTexture();
  updateStats();
  requestRender();
}

function syncGridInputs() {
  columnsInput.value = `${state.columns}`;
  rowsInput.value = `${state.rows}`;
}

function render() {
  syncActiveCanvas();
  if (state.viewMode === "3d") {
    volumeView.render();
  } else {
    atlasView.render();
  }
}

function resetView() {
  if (!state.image) {
    return;
  }

  if (state.viewMode === "3d") {
    volumeView.resetCamera();
  } else {
    atlasView.fitToViewport();
  }
  requestRender();
}

function onImageLoaded() {
  const suggestedGrid = suggestGridDimensions(state.image.width, state.image.height);
  state.columns = suggestedGrid.columns;
  state.rows = suggestedGrid.rows;
  syncGridInputs();
  volumeView.updateTexture();
  atlasView.fitToViewport();
  updateStats();
  dropHint.classList.add("hidden");
  requestRender();
}

imageInput.addEventListener("change", (event) => {
  handleFile(event.target.files?.[0] ?? null, onImageLoaded);
});

columnsInput.addEventListener("input", () => {
  state.columns = Math.max(1, Number.parseInt(columnsInput.value, 10) || 1);
  updateStats();
  requestRender();
});

rowsInput.addEventListener("input", () => {
  state.rows = Math.max(1, Number.parseInt(rowsInput.value, 10) || 1);
  updateStats();
  requestRender();
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
  refreshImageDerivedState();
});

lowThresholdInput.addEventListener("input", () => {
  state.lowThreshold = Number.parseInt(lowThresholdInput.value, 10);
  clampThresholds();
  lowThresholdInput.value = `${state.lowThreshold}`;
  highThresholdInput.value = `${state.highThreshold}`;
  setThresholdLabels();
  updateStats();
  requestRender();
});

highThresholdInput.addEventListener("input", () => {
  state.highThreshold = Number.parseInt(highThresholdInput.value, 10);
  clampThresholds();
  lowThresholdInput.value = `${state.lowThreshold}`;
  highThresholdInput.value = `${state.highThreshold}`;
  setThresholdLabels();
  updateStats();
  requestRender();
});

fitButton.addEventListener("click", resetView);
resetButton.addEventListener("click", resetView);

function beginDrag(event) {
  state.isDragging = true;
  state.dragStartX = event.clientX;
  state.dragStartY = event.clientY;
  if (state.viewMode === "2d") {
    state.dragStartX = event.clientX - state.offsetX;
    state.dragStartY = event.clientY - state.offsetY;
  }
  const activeCanvas = state.viewMode === "3d" ? volumeCanvas : atlasCanvas;
  activeCanvas.classList.add("is-panning");
}

function moveDrag(event) {
  if (state.isDragging) {
    if (state.viewMode === "3d") {
      const deltaX = event.clientX - state.dragStartX;
      const deltaY = event.clientY - state.dragStartY;
      state.dragStartX = event.clientX;
      state.dragStartY = event.clientY;
      volumeView.orbit(deltaX, deltaY);
    } else {
      state.offsetX = event.clientX - state.dragStartX;
      state.offsetY = event.clientY - state.dragStartY;
    }
    requestRender();
    return;
  }

  if (state.viewMode === "2d") {
    atlasView.updateHover(event);
    updateStats();
    requestRender();
  }
}

function endDrag() {
  state.isDragging = false;
  atlasCanvas.classList.remove("is-panning");
  volumeCanvas.classList.remove("is-panning");
}

function handleWheel(event) {
  if (!state.image) {
    return;
  }

  event.preventDefault();

  if (state.viewMode === "3d") {
    volumeView.dolly(event.deltaY);
    requestRender();
    return;
  }

  const rect = atlasCanvas.getBoundingClientRect();
  const mouseX = event.clientX - rect.left;
  const mouseY = event.clientY - rect.top;
  const zoomFactor = event.deltaY < 0 ? 1.1 : 0.9;
  const nextScale = clamp(state.viewportScale * zoomFactor, state.minScale, state.maxScale);
  const originX = (mouseX - state.offsetX) / state.viewportScale;
  const originY = (mouseY - state.offsetY) / state.viewportScale;
  state.viewportScale = nextScale;
  state.offsetX = mouseX - originX * state.viewportScale;
  state.offsetY = mouseY - originY * state.viewportScale;
  requestRender();
}

atlasCanvas.addEventListener("mousedown", beginDrag);
volumeCanvas.addEventListener("mousedown", beginDrag);
window.addEventListener("mousemove", moveDrag);
window.addEventListener("mouseup", endDrag);

atlasCanvas.addEventListener("mouseleave", () => {
  if (!state.isDragging && state.viewMode === "2d") {
    state.hoveredCell = null;
    state.hoveredValue = null;
    updateStats();
    requestRender();
  }
});

atlasCanvas.addEventListener("wheel", handleWheel, { passive: false });
volumeCanvas.addEventListener("wheel", handleWheel, { passive: false });

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
  handleFile(event.dataTransfer?.files?.[0] ?? null, onImageLoaded);
});

window.addEventListener("resize", resizeViews);

setThresholdLabels();
resizeViews();
updateStats();
