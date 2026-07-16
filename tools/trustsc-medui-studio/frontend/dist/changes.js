// Pure document diffing for the wave-S14 guard rails: what changed in the in-memory AST versus
// the loaded file, which of those changes touch golden-reference evidence (safety-critical
// nodes, and any positioned node — those get automatic golden Bounds references per ADR-014),
// and therefore whether the "CI re-approval required" banner must show. Wave S15 reuses the same
// entries as the proposal summary.
function snapshot(screen) {
    const nodes = new Map();
    const rows = new Map();
    for (const item of screen.items) {
        if (item.type === "Component") {
            nodes.set(item.id, item);
        }
        else {
            rows.set(item.id, item);
            for (const child of item.children) {
                nodes.set(child.id, child);
            }
        }
    }
    return { nodes, rows };
}
/** Structural equality via JSON. Sound here because both sides share construction lineage: the
 * "before" objects come from one server parse and every edit spreads them field-for-field, so key
 * order never diverges between the two versions being compared. */
function same(a, b) {
    return JSON.stringify(a) === JSON.stringify(b);
}
function nodeHasGoldenEvidence(node) {
    return node.safety_critical !== null || node.position !== null;
}
function nodeEntry(id, before, after) {
    if (before && !after) {
        return {
            id,
            change: "removed",
            geometryChanged: false,
            safetyCritical: before.safety_critical !== null,
            goldenAffected: nodeHasGoldenEvidence(before),
        };
    }
    if (!before && after) {
        return {
            id,
            change: "added",
            geometryChanged: false,
            safetyCritical: after.safety_critical !== null,
            goldenAffected: nodeHasGoldenEvidence(after),
        };
    }
    if (!before || !after || same(before, after)) {
        return null;
    }
    const geometryChanged = !same(before.position, after.position) || !same(before.width, after.width) || !same(before.height, after.height);
    const annotationChanged = !same(before.safety_critical, after.safety_critical);
    const safetyCritical = before.safety_critical !== null || after.safety_critical !== null;
    return {
        id,
        change: "modified",
        geometryChanged,
        safetyCritical,
        goldenAffected: annotationChanged || (geometryChanged && (safetyCritical || before.position !== null || after.position !== null)),
    };
}
function rowEntry(id, before, after) {
    if (before && !after) {
        return { id, change: "removed", geometryChanged: false, safetyCritical: false, goldenAffected: false };
    }
    if (!before && after) {
        return { id, change: "added", geometryChanged: false, safetyCritical: false, goldenAffected: false };
    }
    if (!before || !after) {
        return null;
    }
    // Children are diffed as nodes; only the Row's own fields matter here.
    const ownBefore = { ...before, children: [] };
    const ownAfter = { ...after, children: [] };
    if (same(ownBefore, ownAfter)) {
        return null;
    }
    return {
        id,
        change: "modified",
        geometryChanged: !same(before.height, after.height) || before.spacing !== after.spacing,
        safetyCritical: false,
        goldenAffected: false,
    };
}
/** Diffs `current` against the loaded `initial` document, entry per changed node/Row (matched by
 * id — a rename therefore reads as removed + added, which is also what its golden evidence does). */
export function diffScreens(initial, current) {
    const before = snapshot(initial);
    const after = snapshot(current);
    const entries = [];
    const nodeIds = new Set([...before.nodes.keys(), ...after.nodes.keys()]);
    for (const id of nodeIds) {
        const entry = nodeEntry(id, before.nodes.get(id), after.nodes.get(id));
        if (entry) {
            entries.push(entry);
        }
    }
    const rowIds = new Set([...before.rows.keys(), ...after.rows.keys()]);
    for (const id of rowIds) {
        const entry = rowEntry(id, before.rows.get(id), after.rows.get(id));
        if (entry) {
            entries.push(entry);
        }
    }
    const screenChanged = !same(initial.layout, current.layout) || !same(initial.declared_surface, current.declared_surface);
    return { entries, screenChanged };
}
/** True when any change in the diff invalidates golden references / ColorHash baselines — the
 * condition for the persistent "CI re-approval required" banner. */
export function hasGoldenImpact(diff) {
    return diff.entries.some((entry) => entry.goldenAffected);
}
/** One human-readable line for a change entry — shared by the wave-S14 changes drawer and the
 * wave-S15 proposal dialog's prefilled description. */
export function describeChange(entry) {
    const verb = entry.change === "added"
        ? "added"
        : entry.change === "removed"
            ? "removed"
            : entry.geometryChanged
                ? "moved/resized"
                : "edited";
    const flags = `${entry.safetyCritical ? " \u{1F6E1} safety-critical" : ""}${entry.goldenAffected ? " ⚠ golden references affected" : ""}`;
    return `${verb} ${entry.id}${flags}`;
}
/** The total number of changed items a diff represents — nodes/Rows plus the screen-level
 * layout/surface change, if any. Used to gate the "Propose change" button and label the drawer. */
export function changeCount(diff) {
    return diff.entries.length + (diff.screenChanged ? 1 : 0);
}
