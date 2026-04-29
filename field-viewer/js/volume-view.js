import { clamp, getChannelIndex, getCellDimensions, getVolumeDimensions, state } from "./state.js";

const VERTEX_SHADER_SOURCE = `#version 300 es
in vec2 aPosition;
out vec2 vUv;

void main() {
  vUv = aPosition * 0.5 + 0.5;
  gl_Position = vec4(aPosition, 0.0, 1.0);
}
`;

const FRAGMENT_SHADER_SOURCE = `#version 300 es
precision highp float;

in vec2 vUv;
out vec4 outColor;

uniform sampler2D uAtlas;
uniform vec2 uAtlasSize;
uniform vec2 uGrid;
uniform vec3 uVolumeSize;
uniform float uAspect;
uniform float uYaw;
uniform float uPitch;
uniform float uCameraDistance;
uniform vec2 uThreshold;
uniform int uChannel;
uniform int uDataMode;
uniform float uStepCount;

vec3 rotateX(vec3 v, float angle) {
  float c = cos(angle);
  float s = sin(angle);
  return vec3(v.x, v.y * c - v.z * s, v.y * s + v.z * c);
}

vec3 rotateY(vec3 v, float angle) {
  float c = cos(angle);
  float s = sin(angle);
  return vec3(v.x * c + v.z * s, v.y, -v.x * s + v.z * c);
}

vec3 inverseRotate(vec3 v) {
  return rotateY(rotateX(v, -uPitch), -uYaw);
}

bool intersectBox(vec3 origin, vec3 direction, out float tNear, out float tFar) {
  vec3 boxMin = vec3(-1.0);
  vec3 boxMax = vec3(1.0);
  vec3 invDir = 1.0 / direction;
  vec3 t0 = (boxMin - origin) * invDir;
  vec3 t1 = (boxMax - origin) * invDir;
  vec3 tsmaller = min(t0, t1);
  vec3 tbigger = max(t0, t1);
  tNear = max(max(tsmaller.x, tsmaller.y), tsmaller.z);
  tFar = min(min(tbigger.x, tbigger.y), tbigger.z);
  return tFar >= max(tNear, 0.0);
}

float readChannel(vec4 voxel) {
  if (uChannel == 0) return voxel.r * 255.0;
  if (uChannel == 1) return voxel.g * 255.0;
  if (uChannel == 2) return voxel.b * 255.0;
  return voxel.a * 255.0;
}

vec4 sampleAtlas(vec3 volumeCoord) {
  float depth = uVolumeSize.z;
  float slice = min(depth - 1.0, floor(volumeCoord.z * depth));
  float cellWidth = uVolumeSize.x;
  float cellHeight = uVolumeSize.y;
  float cellCol = mod(slice, uGrid.x);
  float cellRow = floor(slice / uGrid.x);

  vec2 pixel = vec2(
    cellCol * cellWidth + volumeCoord.x * max(cellWidth - 1.0, 0.0) + 0.5,
    cellRow * cellHeight + volumeCoord.y * max(cellHeight - 1.0, 0.0) + 0.5
  );

  return texture(uAtlas, pixel / uAtlasSize);
}

vec4 fetchVoxel(ivec3 voxelCoord) {
  int x = clamp(voxelCoord.x, 0, int(uVolumeSize.x) - 1);
  int y = clamp(voxelCoord.y, 0, int(uVolumeSize.y) - 1);
  int z = clamp(voxelCoord.z, 0, int(uVolumeSize.z) - 1);
  int sliceCol = z % int(uGrid.x);
  int sliceRow = z / int(uGrid.x);
  ivec2 pixel = ivec2(
    sliceCol * int(uVolumeSize.x) + x,
    sliceRow * int(uVolumeSize.y) + y
  );

  return texelFetch(uAtlas, pixel, 0);
}

vec3 channelColor(int channel) {
  if (channel == 0) return vec3(1.0, 0.48, 0.48);
  if (channel == 1) return vec3(0.44, 1.0, 0.66);
  if (channel == 2) return vec3(0.47, 0.67, 1.0);
  return vec3(1.0, 0.87, 0.52);
}

float readChannelAt(vec3 volumeCoord) {
  return readChannel(sampleAtlas(clamp(volumeCoord, vec3(0.0), vec3(0.999999))));
}

float readGeneratorFieldAt(vec3 volumeCoord) {
  vec3 clampedCoord = clamp(volumeCoord, vec3(0.0), vec3(0.999999));
  vec3 voxelPosition = clampedCoord * (uVolumeSize - vec3(1.0));
  ivec3 base = ivec3(floor(voxelPosition));
  vec3 fraction = fract(voxelPosition);
  float weightedValue = 0.0;
  float totalWeight = 0.0;

  for (int z = 0; z <= 1; z += 1) {
    for (int y = 0; y <= 1; y += 1) {
      for (int x = 0; x <= 1; x += 1) {
        ivec3 offset = ivec3(x, y, z);
        vec3 selector = vec3(float(x), float(y), float(z));
        vec3 axisWeight = mix(vec3(1.0) - fraction, fraction, selector);
        float weight = axisWeight.x * axisWeight.y * axisWeight.z;
        vec4 voxel = fetchVoxel(base + offset);
        float occupancy = step(0.5, voxel.g);

        weightedValue += voxel.r * 255.0 * weight * occupancy;
        totalWeight += weight * occupancy;
      }
    }
  }

  if (totalWeight <= 0.0001) {
    return -1.0;
  }

  return weightedValue / totalWeight;
}

float readFieldAt(vec3 volumeCoord) {
  if (uDataMode == 1) {
    return readGeneratorFieldAt(volumeCoord);
  }

  return readChannelAt(volumeCoord);
}

vec3 estimateNormal(vec3 volumeCoord) {
  vec3 texel = vec3(
    1.0 / max(uVolumeSize.x, 1.0),
    1.0 / max(uVolumeSize.y, 1.0),
    1.0 / max(uVolumeSize.z, 1.0)
  );

  float center = max(readFieldAt(volumeCoord), 0.0);
  float sampleXp = max(readFieldAt(volumeCoord + vec3(texel.x, 0.0, 0.0)), center);
  float sampleXm = max(readFieldAt(volumeCoord - vec3(texel.x, 0.0, 0.0)), center);
  float sampleYp = max(readFieldAt(volumeCoord + vec3(0.0, texel.y, 0.0)), center);
  float sampleYm = max(readFieldAt(volumeCoord - vec3(0.0, texel.y, 0.0)), center);
  float sampleZp = max(readFieldAt(volumeCoord + vec3(0.0, 0.0, texel.z)), center);
  float sampleZm = max(readFieldAt(volumeCoord - vec3(0.0, 0.0, texel.z)), center);

  vec3 gradient = vec3(sampleXp - sampleXm, sampleYp - sampleYm, sampleZp - sampleZm);
  float gradientLength = length(gradient);
  if (gradientLength < 0.0001) {
    return vec3(0.0, 0.0, 1.0);
  }

  return normalize(-gradient);
}

void main() {
  vec3 backgroundTop = vec3(0.08, 0.13, 0.16);
  vec3 backgroundBottom = vec3(0.03, 0.04, 0.06);
  vec3 background = mix(backgroundTop, backgroundBottom, vUv.y);

  float screenX = (vUv.x * 2.0 - 1.0) * uAspect * 1.15;
  float screenY = (1.0 - vUv.y * 2.0) * 1.15;

  vec3 origin = inverseRotate(vec3(0.0, 0.0, uCameraDistance));
  vec3 direction = normalize(inverseRotate(vec3(screenX, screenY, -1.0)));

  float tNear;
  float tFar;
  if (!intersectBox(origin, direction, tNear, tFar)) {
    outColor = vec4(background, 1.0);
    return;
  }

  float startT = max(tNear, 0.0);
  float totalDistance = tFar - startT;
  float dt = totalDistance / uStepCount;
  vec3 baseColor = uDataMode == 1 ? vec3(0.48, 0.82, 1.0) : channelColor(uChannel);
  float thresholdMid = (uThreshold.x + uThreshold.y) * 0.5;
  float thresholdHalfSpan = max((uThreshold.y - uThreshold.x) * 0.5, 0.5);
  float previousSignedDistance = 0.0;
  bool hasPreviousSample = false;
  bool hitSurface = false;
  vec3 shadedColor = background;

  for (float i = 0.0; i < 512.0; i += 1.0) {
    if (i >= uStepCount) {
      break;
    }

    float t = startT + dt * i;
    vec3 position = origin + direction * t;
    vec3 volumeCoord = position * 0.5 + 0.5;
    float value = readFieldAt(volumeCoord);
    if (value < 0.0) {
      previousSignedDistance = 1.0;
      hasPreviousSample = true;
      continue;
    }
    float signedDistance = abs(value - thresholdMid) - thresholdHalfSpan;

    if (signedDistance <= 0.0) {
      float refinedNear = max(startT, t - dt);
      float refinedFar = t;
      float nearSignedDistance = hasPreviousSample ? previousSignedDistance : signedDistance;

      for (int refineStep = 0; refineStep < 5; refineStep += 1) {
        float midpoint = (refinedNear + refinedFar) * 0.5;
        vec3 midpointPosition = origin + direction * midpoint;
        vec3 midpointCoord = midpointPosition * 0.5 + 0.5;
        float midpointValue = readFieldAt(midpointCoord);
        if (midpointValue < 0.0) {
          refinedNear = midpoint;
          nearSignedDistance = 1.0;
          continue;
        }
        float midpointSignedDistance = abs(midpointValue - thresholdMid) - thresholdHalfSpan;
        if (midpointSignedDistance <= 0.0) {
          refinedFar = midpoint;
        } else {
          refinedNear = midpoint;
          nearSignedDistance = midpointSignedDistance;
        }
      }

      vec3 hitPosition = origin + direction * refinedFar;
      vec3 hitCoord = hitPosition * 0.5 + 0.5;
      vec3 normal = estimateNormal(hitCoord);
      vec3 lightDirection = normalize(vec3(-0.45, 0.7, 0.55));
      vec3 viewDirection = normalize(origin - hitPosition);
      vec3 halfVector = normalize(lightDirection + viewDirection);
      float diffuse = max(dot(normal, lightDirection), 0.0);
      float specular = pow(max(dot(normal, halfVector), 0.0), 28.0);
      float fresnel = pow(1.0 - max(dot(normal, viewDirection), 0.0), 3.0);
      float ambient = 0.22;
      float rim = fresnel * 0.28;
      vec3 surfaceColor = baseColor * (ambient + diffuse * 0.95) + vec3(specular * 0.42 + rim);
      shadedColor = mix(background * 0.55, surfaceColor, 0.95);
      hitSurface = true;
      break;
    }

    previousSignedDistance = signedDistance;
    hasPreviousSample = true;
  }

  if (hitSurface) {
    outColor = vec4(shadedColor, 1.0);
    return;
  }

  outColor = vec4(background, 1.0);
}
`;

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

function createProgram(gl) {
  const vertexShader = compileShader(gl, gl.VERTEX_SHADER, VERTEX_SHADER_SOURCE);
  const fragmentShader = compileShader(gl, gl.FRAGMENT_SHADER, FRAGMENT_SHADER_SOURCE);
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

export class VolumeView {
  constructor(canvas, viewport) {
    this.canvas = canvas;
    this.viewport = viewport;
    this.gl = canvas.getContext("webgl2", {
      alpha: true,
      antialias: true,
      depth: false,
      stencil: false,
      premultipliedAlpha: false,
    });

    if (!this.gl) {
      throw new Error("WebGL2 is not available in this browser.");
    }

    this.program = createProgram(this.gl);
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
