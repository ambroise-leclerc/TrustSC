// Pure AST helpers (ADR-022 wave S11): finding, updating, and interpreting nodes in the
// client-held ScreenDefinitionDto document. No DOM here on purpose — editor.ts owns the
// interactive/DOM side, this file is the part that's actually worth reasoning about in
// isolation (and the part a future test runner, if one lands, would target first).
/** True only for nodes the canvas can drag/resize: the parser requires fixed `width`/`height`
 * whenever `position:` is present (see `parse_component_properties` in the governed crate), so
 * "absolutely positioned" and "fixed px dimensions" are the same condition in practice — both
 * are still checked explicitly since a client-constructed AST could in principle violate that
 * pairing (e.g. mid-edit) even though a compiled one never does. */
export function isDraggable(node) {
    return node.position !== null && node.width.kind === "Px" && node.height.kind === "Px";
}
export function dimensionPx(dimension) {
    return dimension.kind === "Px" ? dimension.value : null;
}
/** True for a *compiled* node's kind that names a Row's synthesized background — Panels are
 * compiler-synthesized only (component-dictionary.md: id is always `{row_id}-background`) and
 * never appear in the AST at all, so they can't be looked up via `findNode`. */
export function isSyntheticPanel(kind) {
    return kind.kind === "Panel";
}
const PANEL_ID_SUFFIX = "-background";
/** Recovers the owning Row's id from a synthesized Panel's compiled node id, so clicking a Row's
 * background can select the Row instead (inspector-only in this wave — S13 is the inspector). */
export function rowIdFromPanelId(panelId) {
    return panelId.endsWith(PANEL_ID_SUFFIX) ? panelId.slice(0, -PANEL_ID_SUFFIX.length) : panelId;
}
/** Finds a node by id anywhere in the screen: a top-level `Component` item, or nested in a
 * `Row`'s `children`. Rows themselves aren't `NodeDefinitionDto`s (they have no `kind`/`position`
 * of their own) and are never returned here. */
export function findNode(screen, nodeId) {
    for (const item of screen.items) {
        if (item.type === "Component" && item.id === nodeId) {
            return item;
        }
        if (item.type === "Row") {
            const child = item.children.find((candidate) => candidate.id === nodeId);
            if (child) {
                return child;
            }
        }
    }
    return null;
}
/** Returns a new `ScreenDefinitionDto` with `nodeId`'s node replaced by `updater(node)`, leaving
 * every other node (and the previous `screen` object itself) untouched. A no-op `updater` call
 * (id not found) returns `screen` unchanged, by reference, so callers can cheaply check whether
 * anything actually changed. */
export function updateNode(screen, nodeId, updater) {
    let changed = false;
    const items = screen.items.map((item) => {
        if (item.type === "Component" && item.id === nodeId) {
            changed = true;
            return { ...updater(item), type: "Component" };
        }
        if (item.type === "Row") {
            let rowChanged = false;
            const children = item.children.map((child) => {
                if (child.id === nodeId) {
                    rowChanged = true;
                    return updater(child);
                }
                return child;
            });
            if (rowChanged) {
                changed = true;
                return { ...item, children };
            }
        }
        return item;
    });
    return changed ? { ...screen, items } : screen;
}
/** Grid-snaps a coordinate/length to the nearest multiple of `grid` (8px, matching the examples'
 * coordinate style) unless `disabled` (Shift held), in which case the value passes through
 * unchanged. Always clamped to >= 0 either way: positions/sizes can never go negative. */
export function snap(value, grid, disabled) {
    const snapped = disabled ? value : Math.round(value / grid) * grid;
    return Math.max(0, snapped);
}
/** The bounds a node's *own* AST fields (position + fixed px width/height) describe, independent
 * of any compiled result. Used to draw the "proposed" rect for a node currently being edited when
 * the last compile attempt failed — the last-good compiled bounds are stale by definition once an
 * edit has been proposed on top of them. `null` for a non-draggable (flow) node: it has no
 * absolute position to draw. */
export function proposedBounds(node) {
    if (node.position === null) {
        return null;
    }
    const width = dimensionPx(node.width);
    const height = dimensionPx(node.height);
    if (width === null || height === null) {
        return null;
    }
    const [x, y] = node.position;
    return { x, y, w: width, h: height };
}
/** Wave S11's "convert to absolute" context-menu action: pins a flow node's *current compiled*
 * bounds as its new `position:` + fixed px `width:`/`height:` — visually lossless (the node
 * doesn't move on the next render) and a small, clean diff once the screen is eventually
 * serialized. */
export function convertToAbsolute(screen, nodeId, bounds) {
    return updateNode(screen, nodeId, (node) => ({
        ...node,
        position: [bounds.x, bounds.y],
        width: { kind: "Px", value: bounds.w },
        height: { kind: "Px", value: bounds.h },
    }));
}
