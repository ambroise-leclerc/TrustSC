# Manual test checklist — previewer (wave S9) + canvas editor (wave S11)

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

## Known limits (this wave)

- No save/propose-change flow yet (wave S15): every canvas edit lives only in the browser tab's
  memory and is discarded on reload or navigating away (including a locale switch, which
  re-fetches the screen from disk).
- No inspector (wave S13): a Row's own properties (`spacing:`, `background:`) aren't editable,
  and clicking its background only reports the selection.
- Render latency is the render bridge's own (ADR-022 wave S7): each frame is a fresh Vulkan
  instance, typically ~100-500ms on lavapipe, more under host load. During a drag, only the
  overlay rect moves (no re-render per mouse move, per the wave S11 compile-loop design); once
  compile+render is triggered (on drop, or debounced ~250ms after a keyboard nudge), the
  previous frame image stays visible until the new one is ready to swap in.
