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
/** Finds a top-level Row by id. Row-children lookups go through `findNode`; this is for editing
 * the Row's *own* properties (height, spacing, background) in the inspector (wave S13). */
export function findRow(screen, rowId) {
    for (const item of screen.items) {
        if (item.type === "Row" && item.id === rowId) {
            return item;
        }
    }
    return null;
}
/** `updateNode`'s counterpart for a Row's own definition. Same contract: returns `screen`
 * unchanged, by reference, when the id names no Row. */
export function updateRow(screen, rowId, updater) {
    let changed = false;
    const items = screen.items.map((item) => {
        if (item.type === "Row" && item.id === rowId) {
            changed = true;
            return { ...updater(item), type: "Row" };
        }
        return item;
    });
    return changed ? { ...screen, items } : screen;
}
/** Mirrors the parser's `parse_identifier`: non-empty, ASCII alphanumerics plus `_` and `-`.
 * Used for inline id validation in the inspector, so a bad rename is rejected at the field
 * instead of surfacing as a compile diagnostic on the whole document. */
export function isValidIdentifier(id) {
    return id.length > 0 && /^[A-Za-z0-9_-]+$/.test(id);
}
/** Every identifier already taken in the screen: component ids, Row ids, and Row-children ids.
 * New-node id generation must avoid all of them — the compiler rejects duplicate ids wherever
 * they appear, and a Row id colliding with a component id is just as fatal as two components. */
export function collectIds(screen) {
    const ids = new Set();
    for (const item of screen.items) {
        ids.add(item.id);
        if (item.type === "Row") {
            for (const child of item.children) {
                ids.add(child.id);
            }
        }
    }
    return ids;
}
/** A fresh unique id for a palette-dropped node: the widget kind in kebab-case plus the first
 * free counter (`label-1`, `critical-button-2`, ...), never colliding with any existing id. */
export function generateNodeId(screen, kindName) {
    const prefix = kindName.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
    const taken = collectIds(screen);
    for (let n = 1;; n++) {
        const candidate = `${prefix}-${n}`;
        if (!taken.has(candidate)) {
            return candidate;
        }
    }
}
/** Returns a new screen with `node` appended as a top-level `Component` item. Palette drops are
 * always absolutely positioned, so appending never disturbs the flow layout of existing items. */
export function appendNode(screen, node) {
    return { ...screen, items: [...screen.items, { ...node, type: "Component" }] };
}
/** Returns a new screen with `nodeId`'s node removed — from the top-level items or from a Row's
 * children (Rows themselves are never removed here, even when emptied). Returns `screen`
 * unchanged, by reference, when the id names no node, mirroring `updateNode`'s contract. */
export function removeNode(screen, nodeId) {
    let changed = false;
    const items = [];
    for (const item of screen.items) {
        if (item.type === "Component" && item.id === nodeId) {
            changed = true;
            continue;
        }
        if (item.type === "Row" && item.children.some((child) => child.id === nodeId)) {
            changed = true;
            items.push({ ...item, children: item.children.filter((child) => child.id !== nodeId) });
            continue;
        }
        items.push(item);
    }
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
