import * as THREE from '../vendor/three.module.min.js';
import { OrbitControls } from '../vendor/addons/controls/OrbitControls.js';
import { parseGcode, getColorForType } from './gcode-parser.js';

/**
 * GCODE 3D Viewer
 * Renders toolpaths as variable-radius tubes colored by type.
 */

// --- Constants ---
const FILAMENT_DIAMETER = 1.75; // mm
const FILAMENT_AREA = Math.PI * (FILAMENT_DIAMETER / 2) ** 2;
const RADIUS_SCALE = 1.0; // Visual scale factor to make tubes visible
const TRAVEL_RADIUS = 0.05; // mm, very thin for travel moves
const SEGMENTS_PER_TUBE = 8; // Number of radial segments per tube

// --- State ---
let scene, camera, renderer, controls;
let typeMeshes = new Map(); // type -> { mesh, segmentRanges[] }
let allSegments = [];
let layers = [];
let stats = null;
let visibleUpToSegment = Infinity;
let currentLayer = Infinity;
let animationMode = 'layer'; // 'layer' | 'progressive'
let isPlaying = false;
let playbackSpeed = 1.0;
let animationId = null;
let lastTime = 0;

// --- DOM refs ---
let canvas, dropHint, fileInput;
let layerSlider, layerValue, layerMinLabel, layerMaxLabel;
let statsLabels = {};
let playBtn, pauseBtn, speedSlider, speedValue, seekBar, seekValue;
let modeLayerBtn, modeProgressiveBtn;

// --- Init ---
function init() {
  canvas = document.getElementById('viewport-canvas');
  dropHint = document.getElementById('drop-hint');
  fileInput = document.getElementById('file-input');

  layerSlider = document.getElementById('layer-slider');
  layerValue = document.getElementById('layer-value');
  layerMinLabel = document.getElementById('layer-min');
  layerMaxLabel = document.getElementById('layer-max');

  playBtn = document.getElementById('play-btn');
  pauseBtn = document.getElementById('pause-btn');
  speedSlider = document.getElementById('speed-slider');
  speedValue = document.getElementById('speed-value');
  seekBar = document.getElementById('seek-bar');
  seekValue = document.getElementById('seek-value');
  modeLayerBtn = document.getElementById('mode-layer-btn');
  modeProgressiveBtn = document.getElementById('mode-progressive-btn');

  // Stats
  const statIds = ['total-segments', 'extrusion-segments', 'travel-segments',
    'total-length', 'total-extrusion', 'estimated-time', 'num-layers', 'filename'];
  for (const id of statIds) {
    statsLabels[id] = document.getElementById(id);
  }

  setupThree();
  setupEvents();
}

function setupThree() {
  scene = new THREE.Scene();

  camera = new THREE.PerspectiveCamera(50, canvas.clientWidth / canvas.clientHeight, 0.1, 10000);
  camera.position.set(50, 50, 50);

  renderer = new THREE.WebGLRenderer({
    canvas,
    antialias: true,
    alpha: false,
  });
  renderer.setPixelRatio(window.devicePixelRatio);
  renderer.setSize(canvas.clientWidth, canvas.clientHeight);
  renderer.setClearColor(0x0b1115);

  controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;

  // Lights
  const ambientLight = new THREE.AmbientLight(0xffffff, 0.6);
  scene.add(ambientLight);

  const dirLight = new THREE.DirectionalLight(0xffffff, 0.8);
  dirLight.position.set(50, 100, 50);
  scene.add(dirLight);

  const dirLight2 = new THREE.DirectionalLight(0xffffff, 0.3);
  dirLight2.position.set(-50, 50, -50);
  scene.add(dirLight2);

  // Axes helper
  const axesHelper = new THREE.AxesHelper(20);
  scene.add(axesHelper);

  // Grid
  const gridHelper = new THREE.GridHelper(100, 20, 0x2a3a4a, 0x1a2a3a);
  scene.add(gridHelper);

  window.addEventListener('resize', onResize);
  animate();
}

function onResize() {
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
  renderer.setSize(w, h);
}

function animate(time = 0) {
  animationId = requestAnimationFrame(animate);

  if (isPlaying) {
    const dt = (time - lastTime) / 1000;
    lastTime = time;
    updateAnimation(dt);
  }

  controls.update();
  renderer.render(scene, camera);
}

function updateAnimation(dt) {
  if (animationMode === 'layer') {
    // Advance by layer
    const layersPerSecond = playbackSpeed * 2;
    currentLayer += layersPerSecond * dt;
    if (currentLayer >= layers.length) {
      currentLayer = layers.length - 1;
      stopPlayback();
      return;
    }
    const layerIdx = Math.floor(currentLayer);
    const layer = layers[layerIdx];
    visibleUpToSegment = layer.endSegment;
    layerSlider.value = layerIdx;
    layerValue.textContent = layerIdx;
    seekBar.value = layerIdx / Math.max(layers.length - 1, 1);
    seekValue.textContent = `${layerIdx} / ${layers.length - 1}`;
  } else {
    // Progressive: advance by segment
    const segmentsPerSecond = playbackSpeed * 500;
    visibleUpToSegment += segmentsPerSecond * dt;
    if (visibleUpToSegment >= allSegments.length) {
      visibleUpToSegment = allSegments.length;
      stopPlayback();
      return;
    }
    // Update seek bar
    const progress = visibleUpToSegment / allSegments.length;
    seekBar.value = progress;
    seekValue.textContent = `${Math.round(progress * 100)}%`;
    // Update layer slider to show current layer
    for (let i = layers.length - 1; i >= 0; i--) {
      if (visibleUpToSegment >= layers[i].endSegment) {
        layerSlider.value = i;
        layerValue.textContent = i;
        break;
      }
    }
  }

  updateVisibility();
}

function updateVisibility() {
  // Update draw ranges on all type meshes
  for (const [type, data] of typeMeshes) {
    const { mesh, segmentRanges } = data;
    let totalVisibleVertices = 0;
    for (const range of segmentRanges) {
      if (range.startSegment < visibleUpToSegment) {
        totalVisibleVertices += range.numVertices;
      }
    }
    mesh.geometry.setDrawRange(0, totalVisibleVertices);
  }
}

// --- Geometry Building ---
function buildGeometry() {
  // Clear existing
  for (const [type, data] of typeMeshes) {
    scene.remove(data.mesh);
    data.mesh.geometry.dispose();
    data.mesh.material.dispose();
  }
  typeMeshes.clear();

  if (allSegments.length === 0) return;

  // Group segments by type
  const segmentsByType = new Map();
  for (let i = 0; i < allSegments.length; i++) {
    const seg = allSegments[i];
    const type = seg.type || '';
    if (!segmentsByType.has(type)) {
      segmentsByType.set(type, []);
    }
    segmentsByType.get(type).push({ index: i, seg });
  }

  // Build geometry per type
  for (const [type, segList] of segmentsByType) {
    const color = new THREE.Color(...getColorForType(type));
    const isTravel = type === 'Travel';
    const travelColor = new THREE.Color(0.3, 0.3, 0.3);

    const vertsPerSeg = SEGMENTS_PER_TUBE * 2 + 2;
    const indicesPerSeg = SEGMENTS_PER_TUBE * 4;

    // Pre-allocate typed arrays (worst case: all segments have geometry)
    const positions = new Float32Array(segList.length * vertsPerSeg * 3);
    const normals = new Float32Array(segList.length * vertsPerSeg * 3);
    const colors = new Float32Array(segList.length * vertsPerSeg * 3);
    const indices = new Uint32Array(segList.length * indicesPerSeg);
    const segmentRanges = [];

    let vertIdx = 0; // current vertex index in the arrays
    let idxIdx = 0; // current index into the index array
    let actualSegCount = 0;

    for (const { index, seg } of segList) {
      const dx = seg.x1 - seg.x0;
      const dy = seg.y1 - seg.y0;
      const dz = seg.z1 - seg.z0;
      const length = Math.sqrt(dx * dx + dy * dy + dz * dz);

      if (length < 0.0001) continue;

      let radius, segColor;
      if (isTravel) {
        radius = TRAVEL_RADIUS;
        segColor = travelColor;
      } else {
        const volume = FILAMENT_AREA * seg.e;
        const crossSectionArea = volume / length;
        radius = Math.sqrt(crossSectionArea / Math.PI) * RADIUS_SCALE;
        segColor = color;
      }

      const nx = dx / length;
      const ny = dy / length;
      const nz = dz / length;

      // Find two perpendicular vectors to the segment direction
      let ux, uy, uz;
      if (Math.abs(nz) < 0.999) {
        const cl = Math.sqrt(nx * nx + ny * ny);
        ux = -ny / cl;
        uy = nx / cl;
        uz = 0;
      } else {
        ux = 1;
        uy = 0;
        uz = 0;
      }
      const vx = ny * uz - nz * uy;
      const vy = nz * ux - nx * uz;
      const vz = nx * uy - ny * ux;

      const cx = (seg.x0 + seg.x1) / 2;
      const cy = (seg.y0 + seg.y1) / 2;
      const cz = (seg.z0 + seg.z1) / 2;
      const halfLen = length / 2;

      const vertStart = vertIdx;

      for (let i = 0; i < SEGMENTS_PER_TUBE; i++) {
        const angle = (i / SEGMENTS_PER_TUBE) * Math.PI * 2;
        const cosA = Math.cos(angle);
        const sinA = Math.sin(angle);
        const rx = ux * cosA + vx * sinA;
        const ry = uy * cosA + vy * sinA;
        const rz = uz * cosA + vz * sinA;

        // Bottom ring vertex
        const bi = vertIdx * 3;
        positions[bi] = cx - nx * halfLen + rx * radius;
        positions[bi + 1] = cy - ny * halfLen + ry * radius;
        positions[bi + 2] = cz - nz * halfLen + rz * radius;
        normals[bi] = rx; normals[bi + 1] = ry; normals[bi + 2] = rz;
        colors[bi] = segColor.r; colors[bi + 1] = segColor.g; colors[bi + 2] = segColor.b;
        vertIdx++;

        // Top ring vertex
        const ti = vertIdx * 3;
        positions[ti] = cx + nx * halfLen + rx * radius;
        positions[ti + 1] = cy + ny * halfLen + ry * radius;
        positions[ti + 2] = cz + nz * halfLen + rz * radius;
        normals[ti] = rx; normals[ti + 1] = ry; normals[ti + 2] = rz;
        colors[ti] = segColor.r; colors[ti + 1] = segColor.g; colors[ti + 2] = segColor.b;
        vertIdx++;
      }

      // Side faces
      for (let i = 0; i < SEGMENTS_PER_TUBE; i++) {
        const next = (i + 1) % SEGMENTS_PER_TUBE;
        const b0 = vertStart + i * 2;
        const t0 = vertStart + i * 2 + 1;
        const b1 = vertStart + next * 2;
        const t1 = vertStart + next * 2 + 1;
        indices[idxIdx++] = b0;
        indices[idxIdx++] = b1;
        indices[idxIdx++] = t0;
        indices[idxIdx++] = t0;
        indices[idxIdx++] = b1;
        indices[idxIdx++] = t1;
      }

      // End caps
      const bottomCenter = vertIdx;
      positions[vertIdx * 3] = cx - nx * halfLen;
      positions[vertIdx * 3 + 1] = cy - ny * halfLen;
      positions[vertIdx * 3 + 2] = cz - nz * halfLen;
      normals[vertIdx * 3] = -nx; normals[vertIdx * 3 + 1] = -ny; normals[vertIdx * 3 + 2] = -nz;
      colors[vertIdx * 3] = segColor.r; colors[vertIdx * 3 + 1] = segColor.g; colors[vertIdx * 3 + 2] = segColor.b;
      vertIdx++;

      const topCenter = vertIdx;
      positions[vertIdx * 3] = cx + nx * halfLen;
      positions[vertIdx * 3 + 1] = cy + ny * halfLen;
      positions[vertIdx * 3 + 2] = cz + nz * halfLen;
      normals[vertIdx * 3] = nx; normals[vertIdx * 3 + 1] = ny; normals[vertIdx * 3 + 2] = nz;
      colors[vertIdx * 3] = segColor.r; colors[vertIdx * 3 + 1] = segColor.g; colors[vertIdx * 3 + 2] = segColor.b;
      vertIdx++;

      for (let i = 0; i < SEGMENTS_PER_TUBE; i++) {
        const next = (i + 1) % SEGMENTS_PER_TUBE;
        indices[idxIdx++] = bottomCenter;
        indices[idxIdx++] = vertStart + i * 2;
        indices[idxIdx++] = vertStart + next * 2;
        indices[idxIdx++] = topCenter;
        indices[idxIdx++] = vertStart + next * 2 + 1;
        indices[idxIdx++] = vertStart + i * 2 + 1;
      }

      segmentRanges.push({ startSegment: index, numVertices: vertsPerSeg });
      actualSegCount++;
    }

    if (actualSegCount === 0) continue;

    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(positions.subarray(0, vertIdx * 3), 3));
    geometry.setAttribute('normal', new THREE.BufferAttribute(normals.subarray(0, vertIdx * 3), 3));
    geometry.setAttribute('color', new THREE.BufferAttribute(colors.subarray(0, vertIdx * 3), 3));
    geometry.setIndex(new THREE.BufferAttribute(indices.subarray(0, idxIdx), 1));

    const material = new THREE.MeshStandardMaterial({
      vertexColors: true,
      roughness: 0.6,
      metalness: 0.1,
      side: THREE.DoubleSide,
    });

    const mesh = new THREE.Mesh(geometry, material);
    mesh.frustumCulled = false;
    scene.add(mesh);

    typeMeshes.set(type, { mesh, segmentRanges });
  }

  // Set initial draw range (show all)
  for (const [type, data] of typeMeshes) {
    data.mesh.geometry.setDrawRange(0, data.mesh.geometry.attributes.position.count);
  }

  // Fit camera
  fitCamera();
}

function fitCamera() {
  if (allSegments.length === 0) return;

  const box = new THREE.Box3();
  for (const seg of allSegments) {
    box.expandByPoint(new THREE.Vector3(seg.x0, seg.y0, seg.z0));
    box.expandByPoint(new THREE.Vector3(seg.x1, seg.y1, seg.z1));
  }

  const center = box.getCenter(new THREE.Vector3());
  const size = box.getSize(new THREE.Vector3());
  const maxDim = Math.max(size.x, size.y, size.z);
  const fov = camera.fov * (Math.PI / 180);
  let cameraZ = maxDim / (2 * Math.tan(fov / 2));
  cameraZ *= 1.5; // padding

  camera.position.set(center.x, center.y + cameraZ * 0.5, center.z + cameraZ);
  controls.target.copy(center);
  controls.update();
}

// --- Camera Presets ---
function setCameraPreset(preset) {
  if (allSegments.length === 0) return;

  const box = new THREE.Box3();
  for (const seg of allSegments) {
    box.expandByPoint(new THREE.Vector3(seg.x0, seg.y0, seg.z0));
    box.expandByPoint(new THREE.Vector3(seg.x1, seg.y1, seg.z1));
  }

  const center = box.getCenter(new THREE.Vector3());
  const size = box.getSize(new THREE.Vector3());
  const maxDim = Math.max(size.x, size.y, size.z);

  switch (preset) {
    case 'fit':
      fitCamera();
      break;
    case 'top':
      camera.position.set(center.x, center.y + maxDim * 2, center.z + 0.001);
      controls.target.copy(center);
      controls.update();
      break;
    case 'front':
      camera.position.set(center.x, center.y + maxDim * 0.1, center.z + maxDim * 2);
      controls.target.copy(center);
      controls.update();
      break;
    case 'right':
      camera.position.set(center.x + maxDim * 2, center.y + maxDim * 0.1, center.z);
      controls.target.copy(center);
      controls.update();
      break;
    case 'isometric':
      const dist = maxDim * 1.8;
      camera.position.set(center.x + dist, center.y + dist * 0.6, center.z + dist);
      controls.target.copy(center);
      controls.update();
      break;
  }
}

// --- Playback ---
function startPlayback() {
  isPlaying = true;
  lastTime = performance.now();
  playBtn.classList.add('active');
  pauseBtn.classList.remove('active');
}

function stopPlayback() {
  isPlaying = false;
  playBtn.classList.remove('active');
  pauseBtn.classList.add('active');
}

function resetPlayback() {
  stopPlayback();
  currentLayer = 0;
  visibleUpToSegment = 0;
  layerSlider.value = 0;
  layerValue.textContent = '0';
  seekBar.value = 0;
  seekValue.textContent = '0';
  updateVisibility();
}

// --- File Loading ---
async function loadGcode(file) {
  const text = await file.text();

  // Show loading state
  dropHint.textContent = `Parsing ${file.name}...`;
  dropHint.classList.remove('hidden');

  // Parse (yield to UI)
  await new Promise(resolve => setTimeout(resolve, 10));
  const parsed = parseGcode(text);
  allSegments = parsed.segments;
  layers = parsed.layers;
  stats = parsed.stats;

  updateStats(file.name);
  updateLayerControls();

  // Build geometry (yield to UI so loading message is visible)
  dropHint.textContent = `Building 3D geometry (${allSegments.length.toLocaleString()} segments)...`;
  await new Promise(resolve => setTimeout(resolve, 10));
  buildGeometry();

  // Hide loading hint
  dropHint.classList.add('hidden');

  // Reset playback to show all
  resetPlayback();
  visibleUpToSegment = Infinity;
  currentLayer = layers.length - 1;
  layerSlider.value = layers.length - 1;
  layerValue.textContent = layers.length - 1;
  updateVisibility();
}

function updateStats(filename) {
  if (!stats) return;

  statsLabels['filename'].textContent = filename || '-';
  statsLabels['total-segments'].textContent = stats.totalSegments.toLocaleString();
  statsLabels['extrusion-segments'].textContent = stats.extrusionSegments.toLocaleString();
  statsLabels['travel-segments'].textContent = stats.travelSegments.toLocaleString();
  statsLabels['total-length'].textContent = `${(stats.totalLength / 1000).toFixed(1)} m`;
  statsLabels['total-extrusion'].textContent = `${stats.totalExtrusion.toFixed(1)} mm`;
  statsLabels['estimated-time'].textContent = formatTime(stats.estimatedTime);
  statsLabels['num-layers'].textContent = stats.numLayers.toLocaleString();
}

function formatTime(minutes) {
  if (minutes < 1) return `${(minutes * 60).toFixed(0)}s`;
  if (minutes < 60) return `${minutes.toFixed(1)} min`;
  const h = Math.floor(minutes / 60);
  const m = Math.floor(minutes % 60);
  return `${h}h ${m}m`;
}

function updateLayerControls() {
  if (layers.length === 0) return;

  layerSlider.min = 0;
  layerSlider.max = layers.length - 1;
  layerSlider.value = layers.length - 1;
  layerValue.textContent = layers.length - 1;
  layerMinLabel.textContent = `Layer 0 (Z: ${layers[0].zMin.toFixed(2)} mm)`;
  const lastLayer = layers[layers.length - 1];
  layerMaxLabel.textContent = `Layer ${layers.length - 1} (Z: ${lastLayer.zMax.toFixed(2)} mm)`;
  seekBar.max = layers.length - 1;
}

// --- Events ---
function setupEvents() {
  // File input
  fileInput.addEventListener('change', (e) => {
    if (e.target.files.length > 0) {
      loadGcode(e.target.files[0]);
    }
  });

  // Drag and drop
  const viewport = document.getElementById('viewport');
  viewport.addEventListener('dragover', (e) => {
    e.preventDefault();
    viewport.classList.add('drag-over');
  });
  viewport.addEventListener('dragleave', () => {
    viewport.classList.remove('drag-over');
  });
  viewport.addEventListener('drop', (e) => {
    e.preventDefault();
    viewport.classList.remove('drag-over');
    if (e.dataTransfer.files.length > 0) {
      const file = e.dataTransfer.files[0];
      if (file.name.endsWith('.gcode') || file.name.endsWith('.gco') || file.type === 'text/plain') {
        loadGcode(file);
      }
    }
  });

  // Layer slider
  layerSlider.addEventListener('input', (e) => {
    const layerIdx = parseInt(e.target.value, 10);
    currentLayer = layerIdx;
    layerValue.textContent = layerIdx;
    if (layerIdx < layers.length) {
      visibleUpToSegment = layers[layerIdx].endSegment;
    }
    updateVisibility();
    // Update seek bar
    seekBar.value = layerIdx;
    seekValue.textContent = `${layerIdx} / ${layers.length - 1}`;
  });

  // Seek bar
  seekBar.addEventListener('input', (e) => {
    if (animationMode === 'progressive') {
      const progress = parseFloat(e.target.value);
      visibleUpToSegment = Math.floor(progress * allSegments.length);
      seekValue.textContent = `${Math.round(progress * 100)}%`;
    } else {
      const layerIdx = parseInt(e.target.value, 10);
      currentLayer = layerIdx;
      layerSlider.value = layerIdx;
      layerValue.textContent = layerIdx;
      if (layerIdx < layers.length) {
        visibleUpToSegment = layers[layerIdx].endSegment;
      }
      seekValue.textContent = `${layerIdx} / ${layers.length - 1}`;
    }
    updateVisibility();
  });

  // Play / Pause
  playBtn.addEventListener('click', () => {
    if (visibleUpToSegment >= allSegments.length) {
      resetPlayback();
    }
    startPlayback();
  });
  pauseBtn.addEventListener('click', stopPlayback);

  // Speed
  speedSlider.addEventListener('input', (e) => {
    playbackSpeed = parseFloat(e.target.value);
    speedValue.textContent = `${playbackSpeed.toFixed(1)}x`;
  });

  // Animation mode
  modeLayerBtn.addEventListener('click', () => {
    animationMode = 'layer';
    modeLayerBtn.classList.add('active');
    modeProgressiveBtn.classList.remove('active');
    seekBar.min = 0;
    seekBar.max = layers.length - 1;
    seekBar.step = 1;
    seekBar.value = currentLayer;
    seekValue.textContent = `${currentLayer} / ${layers.length - 1}`;
  });
  modeProgressiveBtn.addEventListener('click', () => {
    animationMode = 'progressive';
    modeProgressiveBtn.classList.add('active');
    modeLayerBtn.classList.remove('active');
    seekBar.min = 0;
    seekBar.max = 100;
    seekBar.step = 0.1;
    const progress = allSegments.length > 0 ? visibleUpToSegment / allSegments.length : 0;
    seekBar.value = progress * 100;
    seekValue.textContent = `${Math.round(progress * 100)}%`;
  });

  // Camera presets
  document.getElementById('fit-btn').addEventListener('click', () => setCameraPreset('fit'));
  document.getElementById('top-btn').addEventListener('click', () => setCameraPreset('top'));
  document.getElementById('front-btn').addEventListener('click', () => setCameraPreset('front'));
  document.getElementById('right-btn').addEventListener('click', () => setCameraPreset('right'));
  document.getElementById('isometric-btn').addEventListener('click', () => setCameraPreset('isometric'));
  document.getElementById('reset-btn').addEventListener('click', () => {
    resetPlayback();
    visibleUpToSegment = Infinity;
    currentLayer = layers.length - 1;
    if (layers.length > 0) {
      layerSlider.value = layers.length - 1;
      layerValue.textContent = layers.length - 1;
    }
    updateVisibility();
    setCameraPreset('fit');
  });

  // Set default mode
  modeLayerBtn.classList.add('active');
}

// --- Entry ---
init();
