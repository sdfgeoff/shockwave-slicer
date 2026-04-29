import { getCellDimensions, state } from "./state.js";
import { getHoveredCellFromCoords, sampleHoveredPixel } from "./image-model.js";

export class AtlasView {
  constructor(canvas, viewport) {
    this.canvas = canvas;
    this.viewport = viewport;
    this.context = canvas.getContext("2d");
  }

  resize() {
    const rect = this.viewport.getBoundingClientRect();
    const pixelRatio = window.devicePixelRatio || 1;
    this.canvas.width = Math.max(1, Math.floor(rect.width * pixelRatio));
    this.canvas.height = Math.max(1, Math.floor(rect.height * pixelRatio));
    this.canvas.style.width = `${rect.width}px`;
    this.canvas.style.height = `${rect.height}px`;
    this.context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
  }

  fitToViewport() {
    if (!state.image) {
      return;
    }

    const rect = this.viewport.getBoundingClientRect();
    const scaleX = rect.width / state.image.width;
    const scaleY = rect.height / state.image.height;
    state.viewportScale = Math.min(scaleX, scaleY) * 0.92;
    state.offsetX = (rect.width - state.image.width * state.viewportScale) / 2;
    state.offsetY = (rect.height - state.image.height * state.viewportScale) / 2;
  }

  render() {
    const rect = this.viewport.getBoundingClientRect();
    this.context.clearRect(0, 0, rect.width, rect.height);

    if (!state.image) {
      return;
    }

    this.context.save();
    this.context.translate(state.offsetX, state.offsetY);
    this.context.scale(state.viewportScale, state.viewportScale);
    this.context.imageSmoothingEnabled = false;
    this.context.drawImage(state.processedCanvas, 0, 0);
    this.context.restore();

    this.drawGrid();
  }

  drawGrid() {
    if (!state.image || !state.showGrid) {
      return;
    }

    const cell = getCellDimensions();
    this.context.save();
    this.context.translate(state.offsetX, state.offsetY);
    this.context.scale(state.viewportScale, state.viewportScale);
    this.context.strokeStyle = "rgba(255, 255, 255, 0.2)";
    this.context.lineWidth = 1 / state.viewportScale;

    for (let column = 0; column <= state.columns; column += 1) {
      const x = column * cell.width;
      this.context.beginPath();
      this.context.moveTo(x, 0);
      this.context.lineTo(x, state.image.height);
      this.context.stroke();
    }

    for (let row = 0; row <= state.rows; row += 1) {
      const y = row * cell.height;
      this.context.beginPath();
      this.context.moveTo(0, y);
      this.context.lineTo(state.image.width, y);
      this.context.stroke();
    }

    if (state.hoveredCell) {
      this.context.strokeStyle = "rgba(103, 213, 181, 0.95)";
      this.context.lineWidth = 2 / state.viewportScale;
      this.context.strokeRect(
        state.hoveredCell.column * cell.width,
        state.hoveredCell.row * cell.height,
        cell.width,
        cell.height,
      );
    }

    this.context.restore();
  }

  getImageCoordinates(event) {
    if (!state.image) {
      return null;
    }

    const rect = this.canvas.getBoundingClientRect();
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

  updateHover(event) {
    const coords = this.getImageCoordinates(event);
    if (!coords) {
      state.hoveredCell = null;
      state.hoveredValue = null;
      return;
    }

    state.hoveredCell = getHoveredCellFromCoords(coords.x, coords.y);
    state.hoveredValue = {
      channel: state.channel,
      value: sampleHoveredPixel(coords.pixelX, coords.pixelY),
    };
  }
}
