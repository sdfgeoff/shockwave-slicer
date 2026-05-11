/**
 * GCODE Parser
 * Parses shockwave-layers GCODE output (and arbitrary GCODE as best effort).
 *
 * Output: { segments, layers, types, stats }
 *   segments: array of { x0,y0,z0, x1,y1,z1, e, type, layer, feedrate }
 *   layers: array of { index, startSegment, endSegment, zMin, zMax }
 *   types: Set of type strings found
 *   stats: { totalSegments, extrusionSegments, travelSegments, totalLength, totalExtrusion, estimatedTime }
 */

const TYPE_COLORS = {
  'Perimeter':    [1.0, 0.4, 0.2],
  'Inner Wall':   [1.0, 0.6, 0.2],
  'Spiral':       [0.2, 0.6, 1.0],
  'Infill':       [0.3, 0.8, 0.4],
  'Top Infill':   [0.4, 0.9, 0.5],
  'Bottom Infill':[0.3, 0.7, 0.3],
  'Support':      [0.6, 0.6, 0.8],
  'Skirt':        [0.8, 0.8, 0.3],
  'Brim':         [0.7, 0.7, 0.2],
  'Travel':       [0.3, 0.3, 0.3],
  '':             [0.5, 0.5, 0.5],  // fallback
};

function getColorForType(type) {
  return TYPE_COLORS[type] || TYPE_COLORS[''];
}

function parseGcode(text) {
  const lines = text.split('\n');
  const segments = [];
  const layers = [];
  const types = new Set();

  let currentLayer = -1;
  let currentType = '';
  let layerStartSegment = 0;
  let layerZMin = Infinity;
  let layerZMax = -Infinity;

  // Current state
  let x = 0, y = 0, z = 0, e = 0, f = 1200;
  let relativeExtrusion = false;
  let relativePositions = false;

  for (let i = 0; i < lines.length; i++) {
    let line = lines[i].trim();

    // Strip inline comments but preserve ;TYPE: and ;LAYER:
    // Comments can be at the start or inline
    let comment = '';
    const commentIdx = line.indexOf(';');
    if (commentIdx >= 0) {
      comment = line.substring(commentIdx + 1).trim();
      line = line.substring(0, commentIdx).trim();
    }

    // Check for layer change
    const layerMatch = comment.match(/LAYER:(\d+)/);
    if (layerMatch) {
      // Save previous layer
      if (currentLayer >= 0 && layerStartSegment < segments.length) {
        layers.push({
          index: currentLayer,
          startSegment: layerStartSegment,
          endSegment: segments.length,
          zMin: layerZMin,
          zMax: layerZMax,
        });
      }
      currentLayer = parseInt(layerMatch[1], 10);
      layerStartSegment = segments.length;
      layerZMin = Infinity;
      layerZMax = -Infinity;
    }

    // Check for type
    const typeMatch = comment.match(/TYPE:(\w+)/);
    if (typeMatch) {
      currentType = typeMatch[1];
      types.add(currentType);
    }

    if (!line) continue;

    // Parse G-code command
    const parts = line.split(/\s+/);
    const cmd = parts[0].toUpperCase();

    if (cmd === 'G90') {
      relativePositions = false;
      relativeExtrusion = false;
      continue;
    }
    if (cmd === 'G91') {
      relativePositions = true;
      relativeExtrusion = true;
      continue;
    }
    if (cmd === 'G28') {
      // Home command - skip
      continue;
    }
    if (cmd === 'M82') {
      relativeExtrusion = false;
      continue;
    }
    if (cmd === 'M83') {
      relativeExtrusion = true;
      continue;
    }

    // Parse parameters
    let nx = null, ny = null, nz = null, ne = null, nf = null;
    for (let j = 1; j < parts.length; j++) {
      const p = parts[j];
      const val = parseFloat(p.substring(1));
      if (isNaN(val)) continue;
      switch (p[0].toUpperCase()) {
        case 'X': nx = val; break;
        case 'Y': ny = val; break;
        case 'Z': nz = val; break;
        case 'E': ne = val; break;
        case 'F': nf = val; break;
      }
    }

    // Apply relative/absolute
    if (nx !== null) nx = relativePositions ? x + nx : nx;
    if (ny !== null) ny = relativePositions ? y + ny : ny;
    if (nz !== null) nz = relativePositions ? z + nz : nz;
    if (ne !== null) ne = relativeExtrusion ? e + ne : ne;
    if (nf !== null) f = nf;

    // Only create a segment if position changed
    const dx = (nx !== null ? nx : x) - x;
    const dy = (ny !== null ? ny : y) - y;
    const dz = (nz !== null ? nz : z) - z;
    const de = (ne !== null ? ne : e) - e;

    if (dx === 0 && dy === 0 && dz === 0 && de === 0) continue;

    const seg = {
      x0: x, y0: y, z0: z,
      x1: nx !== null ? nx : x,
      y1: ny !== null ? ny : y,
      z1: nz !== null ? nz : z,
      e: Math.abs(de),
      type: cmd === 'G0' ? 'Travel' : currentType || (cmd === 'G1' ? '' : ''),
      layer: currentLayer,
      feedrate: f,
    };

    // Update layer Z bounds
    if (currentLayer >= 0) {
      const newZ = nz !== null ? nz : z;
      if (newZ < layerZMin) layerZMin = newZ;
      if (newZ > layerZMax) layerZMax = newZ;
    }

    segments.push(seg);

    // Update state
    if (nx !== null) x = nx;
    if (ny !== null) y = ny;
    if (nz !== null) z = nz;
    if (ne !== null) e = ne;
  }

  // Save last layer
  if (currentLayer >= 0 && layerStartSegment < segments.length) {
    layers.push({
      index: currentLayer,
      startSegment: layerStartSegment,
      endSegment: segments.length,
      zMin: layerZMin,
      zMax: layerZMax,
    });
  }

  // Compute stats
  let totalLength = 0;
  let totalExtrusion = 0;
  let extrusionSegments = 0;
  let travelSegments = 0;
  let estimatedTime = 0;

  for (const seg of segments) {
    const len = Math.sqrt(
      (seg.x1 - seg.x0) ** 2 +
      (seg.y1 - seg.y0) ** 2 +
      (seg.z1 - seg.z0) ** 2
    );
    totalLength += len;
    if (seg.e > 0) {
      totalExtrusion += seg.e;
      extrusionSegments++;
    } else {
      travelSegments++;
    }
    if (seg.feedrate > 0) {
      estimatedTime += len / seg.feedrate;
    }
  }

  return {
    segments,
    layers,
    types,
    stats: {
      totalSegments: segments.length,
      extrusionSegments,
      travelSegments,
      totalLength,
      totalExtrusion,
      estimatedTime,
      numLayers: layers.length,
    },
  };
}

export { parseGcode, getColorForType, TYPE_COLORS };
