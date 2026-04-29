import { getCellDimensions, getChannelIndex, state } from "./state.js";

const sourceCanvas = document.createElement("canvas");
const sourceContext = sourceCanvas.getContext("2d", { willReadFrequently: true });

export function loadImageFromUrl(url, onLoad) {
  const image = new Image();
  image.onload = () => {
    state.image = image;
    sourceCanvas.width = image.width;
    sourceCanvas.height = image.height;
    sourceContext.clearRect(0, 0, image.width, image.height);
    sourceContext.drawImage(image, 0, 0);
    state.imageData = sourceContext.getImageData(0, 0, image.width, image.height);
    state.hoveredCell = null;
    state.hoveredValue = null;
    processImage();
    onLoad();
  };
  image.src = url;
}

export function handleFile(file, onLoad) {
  if (!file || !file.type.startsWith("image/")) {
    return;
  }

  const reader = new FileReader();
  reader.onload = () => loadImageFromUrl(reader.result, onLoad);
  reader.readAsDataURL(file);
}

export function processImage() {
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

export function sampleHoveredPixel(pixelX, pixelY) {
  if (!state.imageData || !state.image) {
    return null;
  }

  const pixelOffset = (pixelY * state.image.width + pixelX) * 4;
  const channelIndex = getChannelIndex(state.channel);
  return state.imageData.data[pixelOffset + channelIndex];
}

export function getHoveredCellFromCoords(x, y) {
  if (!state.image) {
    return null;
  }

  const cell = getCellDimensions();
  const column = Math.min(state.columns - 1, Math.floor(x / cell.width));
  const row = Math.min(state.rows - 1, Math.floor(y / cell.height));
  return { column, row, index: row * state.columns + column };
}
