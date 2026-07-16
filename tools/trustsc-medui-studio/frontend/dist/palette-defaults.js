// Default AST nodes for palette drops (ADR-022 wave S12): every governed property of a freshly
// dropped widget is defaulted to the first entry of its approved set (first text key, first color
// token, first template, first image, `NoOp`, ...), and the node is sized so that default
// compiles at the drop point — an Image at its baked intrinsic size (the compiler requires an
// exact match), text-bearing widgets from the chosen key's worst-case measured locale bounds.
const GRID_PX = 8;
/** Mirrors ASCII_TEXT_GLYPH_SET_ID in the governed crate — the glyph set `charset: AsciiText`
 * resolves to, and the only approved TextInput charset (the serializer rejects any other). */
const ASCII_TEXT_GLYPH_SET_ID = "SET-ASCII-TEXT";
function snapUp(value) {
    return Math.ceil(value / GRID_PX) * GRID_PX;
}
/** A size that fits `info`'s approved text in its *worst-case* locale (plus breathing room, snapped
 * up to the grid) — so the default node passes the compiler's per-locale text-budget check
 * regardless of which locale is being previewed. Never smaller than `minW`×`minH`. */
function sizeForTextKey(info, minW, minH) {
    let width = 0;
    let height = 0;
    for (const entry of info.entries) {
        width = Math.max(width, entry.width_px);
        height = Math.max(height, entry.height_px);
    }
    return [Math.max(minW, snapUp(width + 2 * GRID_PX)), Math.max(minH, snapUp(height + GRID_PX))];
}
/** The text key a drop defaults to: the first key approved in *every* palette locale, falling
 * back to the key with the widest locale coverage. A key missing a locale still compiles (the
 * budget check runs per approved locale) but fails to *render* the moment the previewer switches
 * to that locale — so prefer keys that can't hit that. */
export function defaultTextKey(palette) {
    let best;
    let bestCoverage = -1;
    for (const info of palette.text_keys) {
        const locales = new Set(info.entries.map((entry) => entry.locale));
        const coverage = palette.locales.filter((locale) => locales.has(locale)).length;
        if (coverage === palette.locales.length) {
            return info;
        }
        if (coverage > bestCoverage) {
            best = info;
            bestCoverage = coverage;
        }
    }
    return best;
}
/** Why a widget kind can't be created from the current palette, or `null` if it can. Non-null
 * exactly when `defaultNodeAt` would return `null` — used to disable the palette entry up front
 * with an explanation instead of letting a drop fail. */
export function cannotCreateReason(kindName, palette) {
    const needsText = ["CriticalButton", "Label", "StatusIndicator", "Button"].includes(kindName);
    if (needsText && palette.text_keys.length === 0) {
        return "no approved text keys in the text package";
    }
    const needsColor = [
        "CriticalButton",
        "SignalTrace",
        "Label",
        "NumericDisplay",
        "Button",
        "TextInput",
    ].includes(kindName);
    if (needsColor && palette.colors.length === 0) {
        return "no approved color tokens";
    }
    if (kindName === "NumericDisplay" && palette.templates.length === 0) {
        return "no approved numeric templates in the text packages";
    }
    if (kindName === "Image" && palette.images.length === 0) {
        return "no baked images in this repo";
    }
    return null;
}
/** Builds the complete default node a palette drop inserts: absolute `position` at the given
 * (already grid-snapped) point, a per-kind default size, and every governed property set to the
 * first entry of its approved set. Returns `null` when a required set is empty — see
 * `cannotCreateReason` for the user-facing explanation. */
export function defaultNodeAt(kindName, palette, id, position) {
    if (cannotCreateReason(kindName, palette) !== null) {
        return null;
    }
    // Non-null by the guard above wherever the kind actually uses them; the fallbacks only keep
    // the type checker satisfied on kinds that never read them.
    const textKey = defaultTextKey(palette);
    const color = palette.colors[0]?.token ?? "";
    let kind;
    let size;
    switch (kindName) {
        case "CriticalButton":
            kind = {
                kind: "CriticalButton",
                requirement_id: "REQ-TODO",
                label_text_key: textKey?.string_id ?? "",
                color_token: color,
                on_press: "NoOp",
            };
            size = textKey ? sizeForTextKey(textKey, 240, 64) : [240, 64];
            break;
        case "VulkanViewport":
            kind = { kind: "VulkanViewport", stream_source: "STREAM_SOURCE" };
            size = [320, 240];
            break;
        case "SignalTrace":
            kind = { kind: "SignalTrace", stream_source: "STREAM_SOURCE", color_token: color };
            size = [320, 160];
            break;
        case "Label":
            kind = { kind: "Label", text_key: textKey?.string_id ?? "", color_token: color };
            size = textKey ? sizeForTextKey(textKey, 160, 32) : [160, 32];
            break;
        case "Clock":
            kind = { kind: "Clock", format: "TimeSeconds" };
            size = [240, 48];
            break;
        case "NumericDisplay": {
            const template = palette.templates[0];
            kind = {
                kind: "NumericDisplay",
                requirement_id: "REQ-TODO",
                template_id: template?.id ?? "",
                source: "DATA_SOURCE",
                color_token: color,
            };
            size = [512, 192];
            break;
        }
        case "StatusIndicator":
            kind = {
                kind: "StatusIndicator",
                requirement_id: "REQ-TODO",
                source: "DATA_SOURCE",
                state_text_keys: [textKey?.string_id ?? ""],
                // Optional in the DSL (absent means Neutral for every state), but the AST DTO carries the
                // resolved form, so mirror the compiler's own default explicitly.
                color_tokens: ["Theme.Colors.Neutral"],
            };
            size = textKey ? sizeForTextKey(textKey, 200, 48) : [200, 48];
            break;
        case "Image": {
            const image = palette.images[0];
            if (!image) {
                return null;
            }
            kind = { kind: "Image", image_id: image.id };
            // Never snapped or defaulted: the compiler requires Image bounds to equal the baked
            // intrinsic size exactly.
            size = [image.width, image.height];
            break;
        }
        case "Button":
            kind = {
                kind: "Button",
                label_text_key: textKey?.string_id ?? "",
                color_token: color,
                source: "DATA_SOURCE",
                requirement_id: null,
            };
            size = textKey ? sizeForTextKey(textKey, 240, 64) : [240, 64];
            break;
        case "TextInput":
            kind = {
                kind: "TextInput",
                source: "DATA_SOURCE",
                max_length: 16,
                glyph_set_id: ASCII_TEXT_GLYPH_SET_ID,
                color_token: color,
                requirement_id: null,
            };
            size = [320, 48];
            break;
        default:
            // An unknown kind_name (a widget added to the governed catalog without updating this
            // switch) — refuse rather than fabricate a node the compiler will reject confusingly.
            return null;
    }
    return {
        id,
        width: { kind: "Px", value: size[0] },
        height: { kind: "Px", value: size[1] },
        position,
        kind,
        safety_critical: null,
    };
}
