// Node-bounds overlay: absolutely-positioned transparent divs on top of the rendered `<img>`,
// one per compiled node, positioned as percentages of the image's natural size so they track any
// zoom level without recomputation. Deliberately factored out of app.ts (rather than inlined
// into the screen view) so wave S11's editor can reuse `renderOverlay`/`boundsToStyle` for its
// drag/resize handles instead of rewriting this geometry from scratch.

import type { Bounds, CompiledNodeSummary } from "./api.js";

export interface OverlayOptions {
  /** Draw a distinct outline + badge on nodes with golden-reference evidence. */
  showGoldenOutlines: boolean;
  onHover?: (node: CompiledNodeSummary | null) => void;
}

/** Converts absolute pixel bounds (in the surface's own coordinate space) to a percentage-based
 * CSS position, so the resulting element scales with its positioned ancestor regardless of that
 * ancestor's actual rendered pixel size. */
export function boundsToStyle(bounds: Bounds, surfaceWidth: number, surfaceHeight: number): Partial<CSSStyleDeclaration> {
  return {
    left: `${(bounds.x / surfaceWidth) * 100}%`,
    top: `${(bounds.y / surfaceHeight) * 100}%`,
    width: `${(bounds.w / surfaceWidth) * 100}%`,
    height: `${(bounds.h / surfaceHeight) * 100}%`,
  };
}

function hasGoldenEvidence(node: CompiledNodeSummary): boolean {
  return node.safety_critical || node.golden_checks.length > 0;
}

/** Renders one overlay `<div>` per node into `container` (which must be the positioned element
 * sized exactly to the displayed image — see app.ts's `.frame-stage`). Returns the created
 * elements, node-aligned, so callers can attach further behavior (S11: drag handles) without
 * re-querying the DOM. */
export function renderOverlay(
  container: HTMLElement,
  nodes: CompiledNodeSummary[],
  surfaceWidth: number,
  surfaceHeight: number,
  options: OverlayOptions,
): HTMLElement[] {
  container.querySelectorAll<HTMLElement>(".node-overlay").forEach((el) => el.remove());

  return nodes.map((node) => {
    const el = document.createElement("div");
    el.className = "node-overlay";
    if (options.showGoldenOutlines && hasGoldenEvidence(node)) {
      el.classList.add("node-overlay--golden");
    }
    Object.assign(el.style, boundsToStyle(node.bounds, surfaceWidth, surfaceHeight));
    el.dataset["nodeId"] = node.id;
    el.title = tooltipText(node);

    if (options.showGoldenOutlines && hasGoldenEvidence(node)) {
      const badge = document.createElement("span");
      badge.className = "node-overlay__badge";
      badge.textContent = "\u{1F6E1}"; // shield
      badge.setAttribute("aria-hidden", "true");
      el.appendChild(badge);
    }

    if (options.onHover) {
      el.addEventListener("mouseenter", () => options.onHover?.(node));
      el.addEventListener("mouseleave", () => options.onHover?.(null));
    }

    container.appendChild(el);
    return el;
  });
}

export function tooltipText(node: CompiledNodeSummary): string {
  const golden = node.golden_checks.length > 0 ? ` | golden: ${node.golden_checks.join(", ")}` : "";
  return `${node.id} (${node.kind.kind})\n${node.bounds.w}×${node.bounds.h} at (${node.bounds.x}, ${node.bounds.y})${golden}`;
}
