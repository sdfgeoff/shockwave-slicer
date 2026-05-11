# GCODE Viewer — Design Decisions

Recorded: 2026-05-12

---

## Q1: GCODE Parsing Scope

**Decision:** Target shockwave-layers GCODE first. Support arbitrary GCODE as best effort.

---

## Q2: 3D Rendering Library

**Decision:** Three.js, vendored locally in a `vendor/` folder. No CDN.

---

## Q3: Visual Representation of Toolpaths

**Decision:** Solid tubes with extrusion-dependent thickness (radius proportional to `sqrt(ΔE / segment_length)`). Color by type (perimeter, infill, support, etc.) from `;TYPE:` comments. Travel moves rendered as thin dimmed lines.

---

## Q4: File Delivery Model

**Decision:** File picker + drag-and-drop only. No server needed. Pure client-side static files.

---

## Q5: Animation / Playback

**Decision:** Both modes:
- **Layer-by-layer reveal** — default
- **Progressive path-following** — toggle
- Controls: play/pause, speed slider, seek bar

---

## Q6: Non-Planar Considerations

**Decision:** No special non-planar features. Render as-is.

---

## Q7: Performance Expectations

**Decision:** Full detail always. Tubes at full resolution, no LOD/culling.

---

## Q8: UI Design Consistency

**Decision:** Reuse field-viewer design language. UI polish not a priority, functional first.

---

## Q9: Build Step / Dependencies

**Decision:** Zero build. Functional first. ES modules via `<script type="module">`, vendored Three.js in `vendor/` folder. Open via HTTP server (same as field-viewer).

---

## Q10: Additional Features

**Decision:** Stats panel (segment count, print time, extrusion volume) and camera presets (fit-to-view, top/front/isometric).

---
