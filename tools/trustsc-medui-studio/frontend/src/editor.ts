// Canvas selection, drag/resize, and the compile loop (ADR-022 wave S11). Owns the in-memory AST
// document, the last-good compiled result, and every mouse/keyboard interaction on the frame
// stage; app.ts owns everything outside the stage (toolbar, diagnostics panel container, hover
// label) and is handed callbacks to keep those in sync.

import {
  compileScreen,
  postFrame,
  type CompiledNodeSummary,
  type CompiledSummary,
  type Diagnostic,
  type NodeDefinitionDto,
  type Palette,
  type ScreenDefinitionDto,
} from "./api.js";
import {
  appendNode,
  convertToAbsolute,
  dimensionPx,
  findNode,
  generateNodeId,
  isDraggable,
  isSyntheticPanel,
  proposedBounds,
  removeNode,
  rowIdFromPanelId,
  snap,
  updateNode,
} from "./ast.js";
import { defaultNodeAt } from "./palette-defaults.js";
import { boundsToStyle, renderOverlay } from "./overlay.js";

const GRID_PX = 8;
const COMPILE_DEBOUNCE_MS = 250;

/** The drag payload type palette items set on dragstart and the canvas accepts on drop — a
 * private convention between app.ts's palette panel and this editor, carrying the widget
 * `kind_name`. */
export const WIDGET_DRAG_MIME = "application/x-medui-widget";

export interface CanvasEditorCallbacks {
  onHover(node: CompiledNodeSummary | null): void;
  onDiagnostics(diagnostics: Diagnostic[]): void;
  onSelectionChange(node: NodeDefinitionDto | null): void;
  onRowSelected(rowId: string): void;
  onCompileError(message: string): void;
}

type DragState =
  | {
      kind: "move";
      nodeId: string;
      startClientX: number;
      startClientY: number;
      startPosition: [number, number];
      pending: [number, number] | null;
    }
  | {
      kind: "resize";
      nodeId: string;
      startClientX: number;
      startClientY: number;
      startWidth: number;
      startHeight: number;
      pending: [number, number] | null;
    };

/** Overlay geometry never waits on the `<img>` itself having finished loading: `app.ts` sets
 * `width`/`height` attributes on the frame `<img>` from the compiled surface size, so the
 * browser reserves the correct box (and `.frame-stage`'s `getBoundingClientRect()` is already
 * correct) before the PNG bytes arrive. `renderOverlays()` relies on this to update immediately
 * on a successful compile, without waiting for the slower `refreshFrame()` PNG round trip. */
export class CanvasEditor {
  private screen: ScreenDefinitionDto;
  private compiled: CompiledSummary;
  private selectedNodeId: string | null = null;
  private lastEditedNodeId: string | null = null;
  private lastDiagnostics: Diagnostic[] = [];
  private showGolden = false;
  private compileTimer: ReturnType<typeof setTimeout> | null = null;
  private frameObjectUrl: string | null = null;
  private drag: DragState | null = null;
  private contextMenuEl: HTMLElement | null = null;
  /** Bumped on every compile/frame round trip so a response that arrives after a newer one has
   * already started (rapid drag-drops, a locale switch mid-flight) is detected and dropped
   * instead of overwriting fresher state with stale data. */
  private generation = 0;

  constructor(
    private locale: string,
    private readonly stage: HTMLElement,
    private readonly img: HTMLImageElement,
    initialScreen: ScreenDefinitionDto,
    initialCompiled: CompiledSummary,
    private readonly palette: Palette,
    private readonly callbacks: CanvasEditorCallbacks,
  ) {
    this.screen = initialScreen;
    this.compiled = initialCompiled;
    document.addEventListener("keydown", this.onKeyDown);
    document.addEventListener("mousemove", this.onMouseMove);
    document.addEventListener("mouseup", this.onMouseUp);
    document.addEventListener("click", this.onDocumentClick);
    // Palette drops (wave S12). Attached to the stage, not the document, so they die with the
    // stage element on navigation — no teardown needed in destroy().
    this.stage.addEventListener("dragover", (event) => {
      if (event.dataTransfer?.types.includes(WIDGET_DRAG_MIME)) {
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
      }
    });
    this.stage.addEventListener("drop", (event) => {
      const kindName = event.dataTransfer?.getData(WIDGET_DRAG_MIME);
      if (!kindName) {
        return;
      }
      event.preventDefault();
      this.insertNodeFromPalette(kindName, event.clientX, event.clientY);
    });
    this.renderOverlays();
  }

  setShowGoldenOutlines(value: boolean): void {
    this.showGolden = value;
    this.renderOverlays();
  }

  setLocale(locale: string): void {
    this.locale = locale;
    const gen = ++this.generation;
    void this.refreshFrame(gen);
  }

  destroy(): void {
    document.removeEventListener("keydown", this.onKeyDown);
    document.removeEventListener("mousemove", this.onMouseMove);
    document.removeEventListener("mouseup", this.onMouseUp);
    document.removeEventListener("click", this.onDocumentClick);
    if (this.compileTimer) {
      clearTimeout(this.compileTimer);
    }
    if (this.frameObjectUrl) {
      URL.revokeObjectURL(this.frameObjectUrl);
    }
    this.closeContextMenu();
  }

  private renderOverlays(): void {
    const [surfaceWidth, surfaceHeight] = this.compiled.surface;
    const elements = renderOverlay(this.stage, this.compiled.nodes, surfaceWidth, surfaceHeight, {
      showGoldenOutlines: this.showGolden,
      onHover: this.callbacks.onHover,
    });

    elements.forEach((el, index) => {
      const compiledNode = this.compiled.nodes[index];
      if (!compiledNode) {
        return;
      }

      if (isSyntheticPanel(compiledNode.kind)) {
        el.classList.add("node-overlay--panel");
        el.addEventListener("click", (event) => {
          event.stopPropagation();
          this.select(null);
          this.callbacks.onRowSelected(rowIdFromPanelId(compiledNode.id));
        });
        return;
      }

      const astNode = findNode(this.screen, compiledNode.id);
      if (!astNode) {
        return;
      }

      el.classList.add("node-overlay--editable");
      if (astNode.id === this.selectedNodeId) {
        el.classList.add("node-overlay--selected");
      }

      if (isDraggable(astNode)) {
        el.addEventListener("mousedown", (event) => this.startDrag(event, astNode.id));
      } else {
        el.classList.add("node-overlay--flow");
        const badge = document.createElement("span");
        badge.className = "node-overlay__flow-badge";
        badge.textContent = "flow";
        badge.setAttribute("aria-hidden", "true");
        el.appendChild(badge);
      }

      el.addEventListener("click", (event) => {
        event.stopPropagation();
        this.select(astNode.id);
      });
      el.addEventListener("contextmenu", (event) => {
        event.preventDefault();
        event.stopPropagation();
        this.openContextMenu(event.clientX, event.clientY, astNode, compiledNode);
      });

      if (astNode.id === this.selectedNodeId && isDraggable(astNode)) {
        const handle = document.createElement("div");
        handle.className = "node-resize-handle";
        handle.addEventListener("mousedown", (event) => this.startResize(event, astNode.id));
        el.appendChild(handle);
      }
    });

    this.renderOffenderRect(surfaceWidth, surfaceHeight);
  }

  private renderOffenderRect(surfaceWidth: number, surfaceHeight: number): void {
    this.stage.querySelectorAll(".node-overlay--offender").forEach((el) => el.remove());
    if (!this.lastEditedNodeId || this.lastDiagnostics.length === 0) {
      return;
    }
    const node = findNode(this.screen, this.lastEditedNodeId);
    if (!node) {
      return;
    }
    const bounds = proposedBounds(node);
    if (!bounds) {
      return;
    }
    const el = document.createElement("div");
    el.className = "node-overlay node-overlay--offender";
    Object.assign(el.style, boundsToStyle(bounds, surfaceWidth, surfaceHeight));
    this.stage.appendChild(el);
  }

  private select(nodeId: string | null): void {
    this.selectedNodeId = nodeId;
    this.callbacks.onSelectionChange(nodeId ? findNode(this.screen, nodeId) : null);
    this.renderOverlays();
  }

  private onDocumentClick = (event: MouseEvent): void => {
    if (this.contextMenuEl && !this.contextMenuEl.contains(event.target as Node)) {
      this.closeContextMenu();
    }
    const target = event.target instanceof Element ? event.target : null;
    if (!target?.closest(".node-overlay") && this.selectedNodeId !== null) {
      this.select(null);
    }
  };

  private startDrag(event: MouseEvent, nodeId: string): void {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const node = findNode(this.screen, nodeId);
    if (!node || node.position === null) {
      return;
    }
    this.select(nodeId);
    this.drag = {
      kind: "move",
      nodeId,
      startClientX: event.clientX,
      startClientY: event.clientY,
      startPosition: node.position,
      pending: null,
    };
  }

  private startResize(event: MouseEvent, nodeId: string): void {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const node = findNode(this.screen, nodeId);
    if (!node) {
      return;
    }
    const width = dimensionPx(node.width);
    const height = dimensionPx(node.height);
    if (width === null || height === null) {
      return;
    }
    this.drag = {
      kind: "resize",
      nodeId,
      startClientX: event.clientX,
      startClientY: event.clientY,
      startWidth: width,
      startHeight: height,
      pending: null,
    };
  }

  /** Converts a mouse-movement delta from CSS pixels (`clientX`/`clientY`) to the surface's own
   * coordinate space, using the stage's *current rendered size* — correct at any zoom level
   * without the caller needing to know which zoom mode is active. */
  private clientDeltaToSurface(dxClient: number, dyClient: number): [number, number] {
    const rect = this.stage.getBoundingClientRect();
    const [surfaceWidth, surfaceHeight] = this.compiled.surface;
    const scaleX = rect.width === 0 ? 1 : surfaceWidth / rect.width;
    const scaleY = rect.height === 0 ? 1 : surfaceHeight / rect.height;
    return [dxClient * scaleX, dyClient * scaleY];
  }

  /** Converts an absolute client point (e.g. a drop location) to the surface's own coordinate
   * space — the positional counterpart of `clientDeltaToSurface`. */
  private clientPointToSurface(clientX: number, clientY: number): [number, number] {
    const rect = this.stage.getBoundingClientRect();
    const [surfaceWidth, surfaceHeight] = this.compiled.surface;
    const scaleX = rect.width === 0 ? 1 : surfaceWidth / rect.width;
    const scaleY = rect.height === 0 ? 1 : surfaceHeight / rect.height;
    return [(clientX - rect.left) * scaleX, (clientY - rect.top) * scaleY];
  }

  /** Wave S12: inserts a fresh default node of `kindName` with its top-left at the (grid-snapped)
   * drop point, selects it, and runs the compile loop. The drop point is used as-is — if the
   * default-size node overlaps something or escapes the surface there, the S11 diagnostic flow
   * (red proposed-rect outline + diagnostics panel, last-good frame kept) reports it rather than
   * the node being silently relocated. */
  insertNodeFromPalette(kindName: string, clientX: number, clientY: number): void {
    const [rawX, rawY] = this.clientPointToSurface(clientX, clientY);
    const position: [number, number] = [snap(rawX, GRID_PX, false), snap(rawY, GRID_PX, false)];
    const id = generateNodeId(this.screen, kindName);
    const node = defaultNodeAt(kindName, this.palette, id, position);
    if (!node) {
      // Palette entries whose governed sets are empty are rendered disabled (app.ts), so this is
      // only reachable for an unknown kind_name — a catalog/frontend version skew.
      this.callbacks.onCompileError(`cannot create a default ${kindName} node`);
      return;
    }
    this.screen = appendNode(this.screen, node);
    this.lastEditedNodeId = id;
    this.select(id);
    this.scheduleCompile(true);
  }

  /** Wave S12: removes the currently selected node (Delete/Backspace), then recompiles. */
  private deleteSelectedNode(): void {
    if (!this.selectedNodeId) {
      return;
    }
    const next = removeNode(this.screen, this.selectedNodeId);
    if (next === this.screen) {
      return;
    }
    this.screen = next;
    this.lastEditedNodeId = null;
    this.select(null);
    this.scheduleCompile(true);
  }

  private onMouseMove = (event: MouseEvent): void => {
    if (!this.drag) {
      return;
    }
    event.preventDefault();
    const node = findNode(this.screen, this.drag.nodeId);
    const el = this.stage.querySelector<HTMLElement>(`.node-overlay[data-node-id="${CSS.escape(this.drag.nodeId)}"]`);
    if (!node || !el) {
      return;
    }
    const [dx, dy] = this.clientDeltaToSurface(
      event.clientX - this.drag.startClientX,
      event.clientY - this.drag.startClientY,
    );
    const disableSnap = event.shiftKey;
    const [surfaceWidth, surfaceHeight] = this.compiled.surface;

    if (this.drag.kind === "move") {
      const x = snap(this.drag.startPosition[0] + dx, GRID_PX, disableSnap);
      const y = snap(this.drag.startPosition[1] + dy, GRID_PX, disableSnap);
      const width = dimensionPx(node.width) ?? 0;
      const height = dimensionPx(node.height) ?? 0;
      this.drag.pending = [x, y];
      Object.assign(el.style, boundsToStyle({ x, y, w: width, h: height }, surfaceWidth, surfaceHeight));
    } else {
      const width = Math.max(GRID_PX, snap(this.drag.startWidth + dx, GRID_PX, disableSnap));
      const height = Math.max(GRID_PX, snap(this.drag.startHeight + dy, GRID_PX, disableSnap));
      const [x, y] = node.position ?? [0, 0];
      this.drag.pending = [width, height];
      Object.assign(el.style, boundsToStyle({ x, y, w: width, h: height }, surfaceWidth, surfaceHeight));
    }
  };

  private onMouseUp = (): void => {
    if (!this.drag) {
      return;
    }
    const { nodeId, pending, kind } = this.drag;
    this.drag = null;
    if (!pending) {
      return;
    }
    if (kind === "move") {
      const [x, y] = pending;
      this.screen = updateNode(this.screen, nodeId, (node) => ({ ...node, position: [x, y] }));
    } else {
      const [width, height] = pending;
      this.screen = updateNode(this.screen, nodeId, (node) => ({
        ...node,
        width: { kind: "Px", value: width },
        height: { kind: "Px", value: height },
      }));
    }
    this.lastEditedNodeId = nodeId;
    this.scheduleCompile(true);
  };

  private onKeyDown = (event: KeyboardEvent): void => {
    if (!this.selectedNodeId) {
      return;
    }
    const active = document.activeElement;
    if (active && ["INPUT", "SELECT", "TEXTAREA"].includes(active.tagName)) {
      return;
    }
    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      this.deleteSelectedNode();
      return;
    }
    const node = findNode(this.screen, this.selectedNodeId);
    if (!node || !isDraggable(node) || node.position === null) {
      return;
    }

    const step = event.shiftKey ? GRID_PX : 1;
    let dx = 0;
    let dy = 0;
    switch (event.key) {
      case "ArrowLeft":
        dx = -step;
        break;
      case "ArrowRight":
        dx = step;
        break;
      case "ArrowUp":
        dy = -step;
        break;
      case "ArrowDown":
        dy = step;
        break;
      default:
        return;
    }
    event.preventDefault();

    const x = Math.max(0, node.position[0] + dx);
    const y = Math.max(0, node.position[1] + dy);
    const nodeId = this.selectedNodeId;
    this.screen = updateNode(this.screen, nodeId, (n) => ({ ...n, position: [x, y] }));
    this.lastEditedNodeId = nodeId;

    // Immediate visual feedback, same as a drag move — don't wait for the debounced compile.
    const el = this.stage.querySelector<HTMLElement>(`.node-overlay[data-node-id="${CSS.escape(nodeId)}"]`);
    const width = dimensionPx(node.width);
    const height = dimensionPx(node.height);
    if (el && width !== null && height !== null) {
      const [surfaceWidth, surfaceHeight] = this.compiled.surface;
      Object.assign(el.style, boundsToStyle({ x, y, w: width, h: height }, surfaceWidth, surfaceHeight));
    }
    this.scheduleCompile(false);
  };

  private openContextMenu(clientX: number, clientY: number, node: NodeDefinitionDto, compiledNode: CompiledNodeSummary): void {
    this.closeContextMenu();
    if (isDraggable(node)) {
      return; // Only flow nodes need "convert to absolute" — an already-absolute node has nothing to convert.
    }
    const menu = document.createElement("div");
    menu.className = "context-menu";
    menu.style.left = `${clientX}px`;
    menu.style.top = `${clientY}px`;
    const item = document.createElement("button");
    item.type = "button";
    item.textContent = "Convert to absolute";
    item.addEventListener("click", (event) => {
      event.stopPropagation();
      this.screen = convertToAbsolute(this.screen, node.id, compiledNode.bounds);
      this.lastEditedNodeId = node.id;
      this.closeContextMenu();
      this.select(node.id);
      this.scheduleCompile(true);
    });
    menu.appendChild(item);
    document.body.appendChild(menu);
    this.contextMenuEl = menu;
  }

  private closeContextMenu(): void {
    this.contextMenuEl?.remove();
    this.contextMenuEl = null;
  }

  private scheduleCompile(immediate: boolean): void {
    if (this.compileTimer) {
      clearTimeout(this.compileTimer);
      this.compileTimer = null;
    }
    const run = () => {
      void this.compileAndRefresh();
    };
    if (immediate) {
      run();
    } else {
      this.compileTimer = setTimeout(run, COMPILE_DEBOUNCE_MS);
    }
  }

  private async compileAndRefresh(): Promise<void> {
    const gen = ++this.generation;
    let result;
    try {
      result = await compileScreen(this.screen);
    } catch (error) {
      if (gen !== this.generation) {
        return; // A newer compile/edit has since started; this response is stale.
      }
      this.callbacks.onCompileError(error instanceof Error ? error.message : String(error));
      return;
    }
    if (gen !== this.generation) {
      return; // A newer compile/edit has since started; this response is stale.
    }

    if (result.ok && result.compiled) {
      this.compiled = result.compiled;
      this.lastDiagnostics = [];
      this.lastEditedNodeId = null;
      this.callbacks.onDiagnostics([]);
      // Reflect the new compiled bounds (and clear any offender rect) immediately: the frame
      // image itself can take much longer to render than the compile step, and the overlay
      // geometry doesn't depend on that image having finished loading (see the class doc on
      // stage sizing). Waiting for refreshFrame() here would leave the offender rect visibly
      // stale for that whole window, disagreeing with the diagnostics panel it just cleared.
      this.renderOverlays();
      await this.refreshFrame(gen);
    } else {
      this.lastDiagnostics = result.diagnostics;
      this.callbacks.onDiagnostics(result.diagnostics);
      this.renderOverlays();
    }
  }

  private async refreshFrame(gen: number): Promise<void> {
    try {
      const blob = await postFrame(this.screen, this.locale);
      if (gen !== this.generation) {
        return; // A newer compile/edit/locale switch has since started; this frame is stale.
      }
      const url = URL.createObjectURL(blob);
      const previous = this.frameObjectUrl;
      this.img.src = url;
      this.frameObjectUrl = url;
      if (previous) {
        URL.revokeObjectURL(previous);
      }
    } catch (error) {
      if (gen !== this.generation) {
        return; // A newer compile/edit/locale switch has since started; this response is stale.
      }
      // The compile step already reported its own outcome via onDiagnostics; a failure here is
      // purely the render step failing on top of a *successful* compile (e.g. a transient
      // renderer error) — rare, and not worth a second, competing error surface on a read path
      // that already has one. Still surfaced, just through the same channel as a hard failure.
      this.callbacks.onCompileError(error instanceof Error ? error.message : String(error));
    }
  }
}
