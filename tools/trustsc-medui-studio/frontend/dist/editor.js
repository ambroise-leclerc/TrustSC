// Canvas selection, drag/resize, and the compile loop (ADR-022 wave S11). Owns the in-memory AST
// document, the last-good compiled result, and every mouse/keyboard interaction on the frame
// stage; app.ts owns everything outside the stage (toolbar, diagnostics panel container, hover
// label) and is handed callbacks to keep those in sync.
import { compileScreen, postFrame, } from "./api.js";
import { convertToAbsolute, dimensionPx, findNode, isDraggable, isSyntheticPanel, proposedBounds, rowIdFromPanelId, snap, updateNode, } from "./ast.js";
import { boundsToStyle, renderOverlay } from "./overlay.js";
const GRID_PX = 8;
const COMPILE_DEBOUNCE_MS = 250;
/** Overlay geometry never waits on the `<img>` itself having finished loading: `app.ts` sets
 * `width`/`height` attributes on the frame `<img>` from the compiled surface size, so the
 * browser reserves the correct box (and `.frame-stage`'s `getBoundingClientRect()` is already
 * correct) before the PNG bytes arrive. `renderOverlays()` relies on this to update immediately
 * on a successful compile, without waiting for the slower `refreshFrame()` PNG round trip. */
export class CanvasEditor {
    constructor(locale, stage, img, initialScreen, initialCompiled, callbacks) {
        this.locale = locale;
        this.stage = stage;
        this.img = img;
        this.callbacks = callbacks;
        this.selectedNodeId = null;
        this.lastEditedNodeId = null;
        this.lastDiagnostics = [];
        this.showGolden = false;
        this.compileTimer = null;
        this.frameObjectUrl = null;
        this.drag = null;
        this.contextMenuEl = null;
        /** Bumped on every compile/frame round trip so a response that arrives after a newer one has
         * already started (rapid drag-drops, a locale switch mid-flight) is detected and dropped
         * instead of overwriting fresher state with stale data. */
        this.generation = 0;
        this.onDocumentClick = (event) => {
            if (this.contextMenuEl && !this.contextMenuEl.contains(event.target)) {
                this.closeContextMenu();
            }
            const target = event.target instanceof Element ? event.target : null;
            if (!target?.closest(".node-overlay") && this.selectedNodeId !== null) {
                this.select(null);
            }
        };
        this.onMouseMove = (event) => {
            if (!this.drag) {
                return;
            }
            event.preventDefault();
            const node = findNode(this.screen, this.drag.nodeId);
            const el = this.stage.querySelector(`.node-overlay[data-node-id="${CSS.escape(this.drag.nodeId)}"]`);
            if (!node || !el) {
                return;
            }
            const [dx, dy] = this.clientDeltaToSurface(event.clientX - this.drag.startClientX, event.clientY - this.drag.startClientY);
            const disableSnap = event.shiftKey;
            const [surfaceWidth, surfaceHeight] = this.compiled.surface;
            if (this.drag.kind === "move") {
                const x = snap(this.drag.startPosition[0] + dx, GRID_PX, disableSnap);
                const y = snap(this.drag.startPosition[1] + dy, GRID_PX, disableSnap);
                const width = dimensionPx(node.width) ?? 0;
                const height = dimensionPx(node.height) ?? 0;
                this.drag.pending = [x, y];
                Object.assign(el.style, boundsToStyle({ x, y, w: width, h: height }, surfaceWidth, surfaceHeight));
            }
            else {
                const width = Math.max(GRID_PX, snap(this.drag.startWidth + dx, GRID_PX, disableSnap));
                const height = Math.max(GRID_PX, snap(this.drag.startHeight + dy, GRID_PX, disableSnap));
                const [x, y] = node.position ?? [0, 0];
                this.drag.pending = [width, height];
                Object.assign(el.style, boundsToStyle({ x, y, w: width, h: height }, surfaceWidth, surfaceHeight));
            }
        };
        this.onMouseUp = () => {
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
            }
            else {
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
        this.onKeyDown = (event) => {
            if (!this.selectedNodeId) {
                return;
            }
            const active = document.activeElement;
            if (active && ["INPUT", "SELECT", "TEXTAREA"].includes(active.tagName)) {
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
            const el = this.stage.querySelector(`.node-overlay[data-node-id="${CSS.escape(nodeId)}"]`);
            const width = dimensionPx(node.width);
            const height = dimensionPx(node.height);
            if (el && width !== null && height !== null) {
                const [surfaceWidth, surfaceHeight] = this.compiled.surface;
                Object.assign(el.style, boundsToStyle({ x, y, w: width, h: height }, surfaceWidth, surfaceHeight));
            }
            this.scheduleCompile(false);
        };
        this.screen = initialScreen;
        this.compiled = initialCompiled;
        document.addEventListener("keydown", this.onKeyDown);
        document.addEventListener("mousemove", this.onMouseMove);
        document.addEventListener("mouseup", this.onMouseUp);
        document.addEventListener("click", this.onDocumentClick);
        this.renderOverlays();
    }
    setShowGoldenOutlines(value) {
        this.showGolden = value;
        this.renderOverlays();
    }
    setLocale(locale) {
        this.locale = locale;
        const gen = ++this.generation;
        void this.refreshFrame(gen);
    }
    destroy() {
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
    renderOverlays() {
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
            }
            else {
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
    renderOffenderRect(surfaceWidth, surfaceHeight) {
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
    select(nodeId) {
        this.selectedNodeId = nodeId;
        this.callbacks.onSelectionChange(nodeId ? findNode(this.screen, nodeId) : null);
        this.renderOverlays();
    }
    startDrag(event, nodeId) {
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
    startResize(event, nodeId) {
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
    clientDeltaToSurface(dxClient, dyClient) {
        const rect = this.stage.getBoundingClientRect();
        const [surfaceWidth, surfaceHeight] = this.compiled.surface;
        const scaleX = rect.width === 0 ? 1 : surfaceWidth / rect.width;
        const scaleY = rect.height === 0 ? 1 : surfaceHeight / rect.height;
        return [dxClient * scaleX, dyClient * scaleY];
    }
    openContextMenu(clientX, clientY, node, compiledNode) {
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
    closeContextMenu() {
        this.contextMenuEl?.remove();
        this.contextMenuEl = null;
    }
    scheduleCompile(immediate) {
        if (this.compileTimer) {
            clearTimeout(this.compileTimer);
            this.compileTimer = null;
        }
        const run = () => {
            void this.compileAndRefresh();
        };
        if (immediate) {
            run();
        }
        else {
            this.compileTimer = setTimeout(run, COMPILE_DEBOUNCE_MS);
        }
    }
    async compileAndRefresh() {
        const gen = ++this.generation;
        let result;
        try {
            result = await compileScreen(this.screen);
        }
        catch (error) {
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
        }
        else {
            this.lastDiagnostics = result.diagnostics;
            this.callbacks.onDiagnostics(result.diagnostics);
            this.renderOverlays();
        }
    }
    async refreshFrame(gen) {
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
        }
        catch (error) {
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
