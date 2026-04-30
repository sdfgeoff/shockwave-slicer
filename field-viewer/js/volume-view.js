import { clamp, getChannelIndex, getCellDimensions, getVolumeDimensions, state } from "./state.js";

async function loadShaderSource(path) {
  const response = await fetch(path);
  if (!response.ok) {
    throw new Error(`Failed to load shader: ${path}`);
  }
  return response.text();
}

async function createProgram(gl) {
  const vertexSource = await loadShaderSource(new URL("../shaders/volume-vertex.glsl", import.meta.url));
  const fragmentSource = await loadShaderSource(new URL("../shaders/volume-fragment.glsl", import.meta.url));

  const vertexShader = compileShader(gl, gl.VERTEX_SHADER, vertexSource);
  const fragmentShader = compileShader(gl, gl.FRAGMENT_SHADER, fragmentSource);
  const program = gl.createProgram();
  gl.attachShader(program, vertexShader);
  gl.attachShader(program, fragmentShader);
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    const error = gl.getProgramInfoLog(program);
    throw new Error(error);
  }
  return program;
}

function compileShader(gl, type, source) {
  const shader = gl.createShader(type);
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    const error = gl.getShaderInfoLog(shader);
    gl.deleteShader(shader);
    throw new Error(error);
  }
  return shader;
}

export class VolumeView {
  constructor(canvas, viewport) {
    this.canvas = canvas;
    this.viewport = viewport;
  }

  async init() {
    this.gl = this.canvas.getContext("webgl2", {
      alpha: true,
      antialias: true,
      depth: false,
      stencil: false,
      premultipliedAlpha: false,
    });

    if (!this.gl) {
      throw new Error("WebGL2 is not available in this browser.");
    }

    this.program = await createProgram(this.gl);
    this.uniforms = this.getUniformLocations();
    this.texture = this.gl.createTexture();
    this.configureBuffers();
    this.configureTexture();
  }

  getUniformLocations() {
    const gl = this.gl;
    return {
      atlas: gl.getUniformLocation(this.program, "uAtlas"),
      atlasSize: gl.getUniformLocation(this.program, "uAtlasSize"),
      grid: gl.getUniformLocation(this.program, "uGrid"),
      volumeSize: gl.getUniformLocation(this.program, "uVolumeSize"),
      aspect: gl.getUniformLocation(this.program, "uAspect"),
      yaw: gl.getUniformLocation(this.program, "uYaw"),
      pitch: gl.getUniformLocation(this.program, "uPitch"),
      cameraDistance: gl.getUniformLocation(this.program, "uCameraDistance"),
      threshold: gl.getUniformLocation(this.program, "uThreshold"),
      channel: gl.getUniformLocation(this.program, "uChannel"),
      dataMode: gl.getUniformLocation(this.program, "uDataMode"),
      stepCount: gl.getUniformLocation(this.program, "uStepCount"),
    };
  }

  configureBuffers() {
    const gl = this.gl;
    const vao = gl.createVertexArray();
    const buffer = gl.createBuffer();
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(
      gl.ARRAY_BUFFER,
      new Float32Array([
        -1, -1,
        1, -1,
        -1, 1,
        -1, 1,
        1, -1,
        1, 1,
      ]),
      gl.STATIC_DRAW,
    );

    const location = gl.getAttribLocation(this.program, "aPosition");
    gl.enableVertexAttribArray(location);
    gl.vertexAttribPointer(location, 2, gl.FLOAT, false, 0, 0);
    this.vao = vao;
  }

  configureTexture() {
    const gl = this.gl;
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  }

  resize() {
    const rect = this.viewport.getBoundingClientRect();
    const pixelRatio = window.devicePixelRatio || 1;
    this.canvas.width = Math.max(1, Math.floor(rect.width * pixelRatio));
    this.canvas.height = Math.max(1, Math.floor(rect.height * pixelRatio));
    this.canvas.style.width = `${rect.width}px`;
    this.canvas.style.height = `${rect.height}px`;
    this.gl.viewport(0, 0, this.canvas.width, this.canvas.height);
  }

  updateTexture() {
    if (!state.imageData || !state.image) {
      return;
    }

    const gl = this.gl;
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.RGBA,
      state.image.width,
      state.image.height,
      0,
      gl.RGBA,
      gl.UNSIGNED_BYTE,
      state.imageData.data,
    );
  }

  render() {
    const gl = this.gl;
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);

    if (!state.image || !state.imageData) {
      return;
    }

    const cell = getCellDimensions();
    const volume = getVolumeDimensions();
    const pixelRatio = window.devicePixelRatio || 1;
    const rect = this.viewport.getBoundingClientRect();
    const aspect = (rect.width * pixelRatio) / Math.max(rect.height * pixelRatio, 1);
    const stepCount = Math.max(48, Math.min(320, Math.ceil(Math.max(
      volume.width,
      volume.height,
      volume.depth,
    ) * 1.6)));

    gl.useProgram(this.program);
    gl.bindVertexArray(this.vao);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.uniform1i(this.uniforms.atlas, 0);
    gl.uniform2f(this.uniforms.atlasSize, state.image.width, state.image.height);
    gl.uniform2f(this.uniforms.grid, state.columns, state.rows);
    gl.uniform3f(this.uniforms.volumeSize, cell.width, cell.height, volume.depth);
    gl.uniform1f(this.uniforms.aspect, aspect);
    gl.uniform1f(this.uniforms.yaw, state.orbitYaw);
    gl.uniform1f(this.uniforms.pitch, state.orbitPitch);
    gl.uniform1f(this.uniforms.cameraDistance, state.cameraDistance);
    gl.uniform2f(this.uniforms.threshold, state.lowThreshold, state.highThreshold);
    gl.uniform1i(this.uniforms.channel, getChannelIndex(state.channel));
    gl.uniform1i(this.uniforms.dataMode, state.dataMode === "field-occupancy" ? 1 : 0);
    gl.uniform1f(this.uniforms.stepCount, stepCount);
    gl.drawArrays(gl.TRIANGLES, 0, 6);
  }

  resetCamera() {
    state.orbitYaw = -0.75;
    state.orbitPitch = 0.55;
    state.cameraDistance = 2.8;
  }

  orbit(deltaX, deltaY) {
    state.orbitYaw += deltaX * 0.01;
    state.orbitPitch = clamp(state.orbitPitch - deltaY * 0.01, -1.45, 1.45);
  }

  dolly(deltaY) {
    const zoomFactor = deltaY < 0 ? 0.9 : 1.1;
    state.cameraDistance = clamp(state.cameraDistance * zoomFactor, 0.45, 8.0);
  }
}
