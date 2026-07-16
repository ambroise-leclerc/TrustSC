# Manual test checklist — previewer (wave S9) + canvas editor (waves S11–S14)

Run before every release of the previewer. Requires a Vulkan ICD (`sudo apt install
mesa-vulkan-drivers` for lavapipe, the same setup CI uses).

```sh
cargo run -p trustsc-medui-studio -- --repo .
```

Open `http://127.0.0.1:8080/` in a browser and work through:

- [ ] **Screen list**: both example screens are listed (`HelloWorld`, `NeuroSense500`), each with
      its repo-relative path.
- [ ] **Screen view — pixel-exact frame**: opening `NeuroSense500` renders the frame at its
      authored 1920×1080 extent (check via "Zoom: 100%" — the image should not be upscaled or
      blurry).
- [ ] **Hover tooltip**: hovering the acknowledge button (`ack-button`) shows a tooltip with its
      id, kind (`Button`), and pixel bounds.
- [ ] **Locale switcher**: switching the locale dropdown to `fr-FR` re-renders the frame with
      French approved text (e.g. "Profondeur d'anesthésie", "ACQUITTER").
- [ ] **Golden-reference outlines**: toggling "golden-reference outlines" draws a dashed outline
      and a shield badge on every node with golden-reference evidence (safety-critical nodes and
      every positioned node), and toggling it off removes them.
- [ ] **Diagnostics panel**: a committed, compiling screen shows "No diagnostics." — see below for
      the broken-screen case.
- [ ] **Download PNG**: the "Download PNG" button downloads the currently displayed frame
      (same image the `<img>` shows, same locale).
- [ ] **Shareable URL**: copying the browser's URL bar (with its `#screen=...&locale=...` hash)
      into a new tab reproduces the exact same screen + locale view.
- [ ] **Zoom modes**: "Fit" scales the frame to the viewport width; "100%" and "200%" show it at
      1x/2x actual pixels with a scrollable viewport.

## Diagnostics panel on a broken screen

Temporarily point `--repo` at a directory containing a `.medui` file with a deliberate error
(e.g. an unknown color token), then open that screen: the diagnostics panel should list the
compiler's error message and line number (when the parser produced one) instead of a blank page,
and the frame view stays usable (no crash, no blank screen).

## Canvas editor (wave S11)

Open `NeuroSense500`:

- [ ] **Drag with grid snap**: drag `ack-button` a moderate distance to an empty area of the
      canvas. While dragging, the overlay rect follows the pointer live and the underlying frame
      image does *not* re-render (no flicker per mouse move). On drop, the frame re-renders with
      the button visibly moved, "No diagnostics." shows, and the overlay's reported position
      (hover it) is a multiple of 8px even if the drag ended on a non-multiple.
- [ ] **Shift disables snap**: repeat a drag while holding Shift — the dropped position is *not*
      forced to an 8px multiple.
- [ ] **Overlap diagnostic**: drag `ack-button` up onto `patient-id-input`. The diagnostics panel
      shows an overlap message; the frame image stays the *last-good* render (button still shown
      at its old position); the dragged node's *proposed* rect is outlined in red on the canvas.
      Drag it back clear of `patient-id-input` — the diagnostic and the red outline both clear
      immediately (no need to wait for a new frame image to load).
- [ ] **Keyboard nudge**: click `ack-button` to select it (a selection outline + resize handle
      appear), then press arrow keys — 1px per press; Shift+arrow — 8px per press. The status
      line under "Selected:" and the diagnostics panel update the same way a drag would.
- [ ] **Flow node badge + convert-to-absolute**: open `HelloWorld`. Its `CriticalButton`
      (`width: Fill`) shows a small "flow" badge and refuses to drag (cursor shows
      not-allowed). Right-click it → "Convert to absolute" in the context menu → the node's
      current rendered bounds become its new `position:`/fixed `width:`/`height:` (visually
      unchanged) and the flow badge disappears. Dragging it now works like any positioned node.
- [ ] **Resize below text budget**: select `device-title` on `NeuroSense500`, drag its
      bottom-right resize handle to shrink it well below the width its approved text needs. The
      diagnostics panel shows a text-budget-exceeded message with the required vs. available
      dimensions; the frame stays last-good and the shrunk node's proposed rect is outlined red.
- [ ] **Row background click**: click a Row's background area (e.g. the top bar behind
      `device-title`/`wall-clock` on `NeuroSense500`, away from any individual widget) — the
      status line reports the owning Row was selected ("inspector lands in a later wave"), not a
      widget; nothing drags.

## Palette drag-and-drop (wave S12)

Open `NeuroSense500`:

- [ ] **Palette panel**: a "Palette" sidebar lists all 10 governed widget kinds, each showing its
      catalog description as a hover tooltip. None are greyed out on this repo (its default
      packages provide text keys, colors, templates, and a baked image).
- [ ] **Drop a Label on empty space**: drag "Label" from the palette onto an empty area of the
      canvas (e.g. below `ack-button`). A new node appears in the rendered frame with the first
      approved text key and color token, its id is `label-1` (hover it), its position is
      grid-snapped (a multiple of 8px), it is selected (outline + resize handle), and the
      diagnostics panel shows "No diagnostics.".
- [ ] **Unique ids**: drop a second Label — its id is `label-2`.
- [ ] **Drop an Image**: drag "Image" onto empty space — it is created at the baked logo's
      intrinsic size (144×48, hover to check) and compiles.
- [ ] **Drop onto an occupied area**: drag a "Button" directly onto `eeg-dsa` (the large
      viewport). The diagnostics panel shows an overlap message, the new node's proposed rect is
      outlined red at the drop point (not silently relocated), and the frame stays last-good.
      Dragging the red-outlined node to empty space clears the diagnostic and renders it.
- [ ] **Delete key**: select a dropped node and press Delete (or Backspace) — the node disappears
      from the frame on the next render and the selection clears. This also works on committed
      nodes (e.g. `ack-button`); reload the page to restore the file's state.

## Property inspector (wave S13)

Open `NeuroSense500`:

- [ ] **Relabel from the governed dropdown**: select `ack-button`; the inspector shows its
      catalog description, id, geometry, and one editor per property. Change `label` to another
      approved key (e.g. `STR-NS-ALERT`) — the per-locale values and measured px widths show
      under the dropdown, and the frame re-renders with the new text.
- [ ] **Budget flagging before compile**: with `ack-button` still selected, open the `label`
      dropdown — keys whose worst-case locale is wider than the button (e.g. `STR-NS-TITLE`,
      flagged "⚠") are visible as such *before* picking one. Pick one anyway: the compiler's
      text-budget diagnostic appears, the frame stays last-good, and the node's proposed rect is
      outlined red.
- [ ] **Color token**: change `ack-button`'s `color` — the swatch next to the dropdown and the
      re-rendered frame agree on the new color.
- [ ] **Duplicate id rejected inline**: rename `ack-button`'s id to `wall-clock` — an inline
      "already taken" error shows at the field, the value snaps back, and no compile runs (the
      diagnostics panel is untouched). Renaming to a fresh id (e.g. `acknowledge-button`)
      applies, and the selection/status line follows the new id.
- [ ] **Image picker resizes**: select `acme-logo`, pick a different entry (only one image is
      baked in this repo, so re-pick the same one) — the width/height fields snap to the baked
      intrinsic size.
- [ ] **Position and size fields**: with a node selected, edit `position` x/y or width/height
      numerically — the frame re-renders; dragging the node on the canvas updates the same
      fields live.
- [ ] **Row properties**: click the topbar Row's background — the inspector shows the Row's id,
      height, spacing, and a `background` dropdown with a "(none)" entry and swatches. Change
      the background — the topbar retints.
- [ ] **Global view**: click empty space (deselect) — the inspector shows the screen's layout
      spacing/padding and the declared surface. Increase the surface (e.g. 2560×1440) — the
      frame re-renders larger; shrinking it below the content (e.g. 1280×720) produces
      out-of-bounds diagnostics with the last-good frame kept.
- [ ] **StatusIndicator states**: select `system-status` — states and their colors edit as
      paired rows; "+ add state" appends one (Neutral color), "×" removes one, and removing the
      last state is not offered.
- [ ] **Locale switch keeps edits**: move `ack-button`, then switch the locale — the frame
      re-renders in the new locale *with the moved button still moved* (the switch no longer
      reloads the saved file).

## Safety guard rails + undo/redo (wave S14)

Open `NeuroSense500`:

- [ ] **Persistent shield badge**: `sedation-index` (the `@safety_critical` node) wears a shield
      badge even with "golden-reference outlines" toggled off; toggling the outlines on keeps a
      single badge (no doubling).
- [ ] **Golden-impact banner**: drag `sedation-index` a few pixels — a persistent warning banner
      appears above the canvas ("Golden references / lavapipe ColorHash baselines change — CI
      re-approval required") and stays across further edits. It also appears when moving a plain
      positioned node (e.g. `ack-button` — auto golden Bounds references), but *not* after only
      recoloring it.
- [ ] **Changes drawer**: after the moves above, the "Changes vs. loaded file (N)" drawer below
      the diagnostics lists each changed node — `sedation-index` flagged both safety-critical and
      golden-affected. Undoing everything empties the drawer and hides the banner.
- [ ] **cv_check checkboxes**: select `sedation-index` — the inspector shows the
      `@safety_critical` section with Bounds and ColorHash checked. Unchecking both: the second
      uncheck is refused with an inline "at least one CV check" error and the AST stays
      unchanged. Unchecking the annotation master checkbox removes it entirely (the changes
      drawer flags this as golden-affected).
- [ ] **ColorHash not offered on Image**: select `acme-logo`, enable `@safety_critical` — the
      ColorHash checkbox is disabled (the compiler rejects it on Image; Bounds works).
- [ ] **Undo/redo**: perform drag → recolor → palette-drop, then press Ctrl+Z three times — the
      document returns to the exact loaded state (frame matches the original, drawer empty,
      banner gone). Ctrl+Shift+Z (or Ctrl+Y) replays all three. Undo/redo works with nothing
      selected, and while typing in an inspector field Ctrl+Z stays the input's own undo.

## Known limits (this wave)

- No save/propose-change flow yet (wave S15): every canvas edit lives only in the browser tab's
  memory and is discarded on reload or navigating away (a locale switch no longer discards —
  wave S13).
- The changes drawer diffs by id, so a rename reads as removed + added (which is also what
  happens to the node's golden-reference evidence).
- Render latency is the render bridge's own (ADR-022 wave S7): each frame is a fresh Vulkan
  instance, typically ~100-500ms on lavapipe, more under host load. During a drag, only the
  overlay rect moves (no re-render per mouse move, per the wave S11 compile-loop design); once
  compile+render is triggered (on drop, or debounced ~250ms after a keyboard nudge), the
  previous frame image stays visible until the new one is ready to swap in.
