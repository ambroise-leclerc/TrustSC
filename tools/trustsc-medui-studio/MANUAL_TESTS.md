# Manual test checklist — read-only previewer (wave S9)

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

## Known limits (this wave)

- Read-only: no editing, drag/resize, or save. That lands in wave S11 onward.
- Render latency is the render bridge's own (ADR-022 wave S7): each frame is a fresh Vulkan
  instance, typically ~100-500ms on lavapipe, more under host load. The previewer does not show
  a loading spinner during that window — the previous frame simply disappears until the new one
  is ready, since `<img>` swapping is the simplest correct behavior for a read-only view.
