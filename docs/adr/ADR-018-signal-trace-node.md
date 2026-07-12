# ADR-018: SignalTrace node and 2D line-strip pipeline

## Status

Accepted

## Context

ADR-011's `VulkanViewport` reserves a region for the 3D spectral "waterfall" heightfield used by
`examples/class_c_monitor` (a scrolling `rows × bins` grid of intensity samples). The raw physiological
signal behind such a spectral view — for NeuroSense 500, the raw EEG that real depth-of-anesthesia
monitors always display next to the derived index — is a fundamentally different visual and data shape: a
single scrolling 1D line of amplitude-over-time samples, not a 2D intensity field. Forcing a raw-signal
trace through the heightfield primitive (e.g. a one-row-tall grid) would misrepresent the data and
constrain the renderer to a mesh topology built for a different purpose. This ADR adds a dedicated
primitive rather than overloading `VulkanViewport`, following ADR-011's own principle of keeping each
reserved-region contract narrow and specific to what it actually renders.

## Decision

- A new MedUI DSL component kind, `SignalTrace`, compiles into a reserved-region descriptor
  (`CompiledNodeKind::SignalTrace { stream_source, color_token }`) analogous to `VulkanViewport`'s
  `ViewportReservation` — it does not embed arbitrary render logic in the UI package.
- The realtime data plane (`crates/trustsc/src/realtime.rs`) gains a dedicated single-sample ring channel,
  `FrameInputs::push_sample(source, f32)` / `FrameInputs::trace(source)`, distinct from
  `push_row`/`StreamBinding`'s `rows × bins` model. A `SignalTrace` node's sample capacity defaults to
  `DEFAULT_TRACE_SAMPLES` per-trace samples; capacity is fixed at bind time from the compiled node, so
  the runtime channel stays a bounded ring, never a growable buffer.
- The adapter renders `SignalTrace` by reusing the existing flat solid-color shaders
  (`adapters/trustsc-vulkan-winit/shaders/flat.{vert,frag}`) verbatim, through a second pipeline object
  built with `VK_PRIMITIVE_TOPOLOGY_LINE_STRIP` instead of `TRIANGLE_LIST`. The shaders themselves are
  topology-agnostic (position pass-through, interpolated color), so this needs no new GLSL, no new
  shader-baker fixture entry, and no new committed `.spv` — only a second `vkCreateGraphicsPipelines`
  call sharing the flat pipeline's empty layout and vertex format (`FlatVertex { position, color }`).
  A persistently mapped vertex buffer per trace (one `FlatVertex` per ring sample) is rewritten each
  frame from the realtime ring — the same "mapped buffer, rewritten per frame" pattern already used for
  the waterfall's height array and the ADR-015 interactive-widget chrome. Line width is fixed at `1.0`
  in v1 so the pipeline needs no optional Vulkan device feature (`wideLines`) and renders identically on
  the lavapipe software rasterizer used by CI's `--verify-ui` path (ADR-016).
- A `SignalTrace` node participates in `--verify-ui`'s `GoldenBounds`/`InkContainment` checks exactly
  like other reserved regions: its rendered ink must stay within its compiled bounds, but its
  frame-to-frame content is not itself golden-compared (it is live signal data, not approved static
  content).

## Consequences

- Physiological scrolling waveforms (raw EEG, ECG, plethysmograms, ...) get a primitive that matches
  their actual data shape and rendering cost, instead of being shoehorned into the spectral-waterfall
  mesh.
- No new GLSL, shader-baker fixture, or committed `.spv` is needed — the flat pipeline's shaders are
  reused as-is, keeping the shader surface area (and its evidence/CI verification footprint) unchanged.
- The realtime API grows one narrow, bounded channel (`push_sample`/`trace`) rather than a general
  "draw arbitrary geometry" escape hatch — the governed/adapter boundary (ADR-005) and the bounded
  fixed-capacity contract (ADR-003 for text, mirrored here) are preserved.
- A future 2D chart/plot primitive with more than one series or non-line marks would need its own ADR;
  this one covers a single scrolling amplitude trace only.
