// Bridges the palette's PropSchema keys (the `.medui` property spellings: `label`, `color`,
// `states`, ...) to the NodeKindDto union's fields (`label_text_key`, `color_token`,
// `state_text_keys`, ...), so the inspector (ADR-022 wave S13) can generate its editors from the
// governed widget catalog without hard-coding one panel per widget kind. Kept exhaustive over
// `NodeKindDto` by hand — dto.rs and api.ts are the actual contracts; a widget added there
// without a mapping here simply shows no editor for the missing key, it can't corrupt the AST.
/** Reads the kind-level property `key` (schema spelling) from `kind`, or `undefined` when the
 * kind has no such property (`id`/`width`/`height`/`position` live on the node, not here). */
export function getKindProp(kind, key) {
    switch (kind.kind) {
        case "CriticalButton":
            if (key === "requirement")
                return kind.requirement_id;
            if (key === "label")
                return kind.label_text_key;
            if (key === "color")
                return kind.color_token;
            if (key === "on_press")
                return kind.on_press;
            return undefined;
        case "VulkanViewport":
            if (key === "stream_source")
                return kind.stream_source;
            return undefined;
        case "SignalTrace":
            if (key === "stream_source")
                return kind.stream_source;
            if (key === "color")
                return kind.color_token;
            return undefined;
        case "Label":
            if (key === "text")
                return kind.text_key;
            if (key === "color")
                return kind.color_token;
            return undefined;
        case "Clock":
            if (key === "format")
                return kind.format;
            return undefined;
        case "NumericDisplay":
            if (key === "requirement")
                return kind.requirement_id;
            if (key === "template")
                return kind.template_id;
            if (key === "source")
                return kind.source;
            if (key === "color")
                return kind.color_token;
            return undefined;
        case "StatusIndicator":
            if (key === "requirement")
                return kind.requirement_id;
            if (key === "source")
                return kind.source;
            if (key === "states")
                return kind.state_text_keys;
            if (key === "colors")
                return kind.color_tokens;
            return undefined;
        case "Panel":
            // Compiler-synthesized only; the inspector never binds to one (see ast.ts's
            // isSyntheticPanel), so there is nothing to expose.
            return undefined;
        case "Image":
            if (key === "source")
                return kind.image_id;
            return undefined;
        case "Button":
            if (key === "requirement")
                return kind.requirement_id;
            if (key === "label")
                return kind.label_text_key;
            if (key === "color")
                return kind.color_token;
            if (key === "source")
                return kind.source;
            return undefined;
        case "TextInput":
            if (key === "requirement")
                return kind.requirement_id;
            if (key === "source")
                return kind.source;
            if (key === "max_length")
                return kind.max_length;
            if (key === "charset")
                return kind.glyph_set_id;
            if (key === "color")
                return kind.color_token;
            return undefined;
    }
}
/** Returns a copy of `kind` with the kind-level property `key` set to `value`, or `kind` itself,
 * by reference, when the kind has no such property. The casts are sound because the inspector
 * only ever produces `value` from the same PropSchema domain this key declares. */
export function withKindProp(kind, key, value) {
    const str = () => String(value ?? "");
    const optStr = () => (value === null || value === "" ? null : String(value));
    switch (kind.kind) {
        case "CriticalButton":
            if (key === "requirement")
                return { ...kind, requirement_id: str() };
            if (key === "label")
                return { ...kind, label_text_key: str() };
            if (key === "color")
                return { ...kind, color_token: str() };
            if (key === "on_press")
                return { ...kind, on_press: str() };
            return kind;
        case "VulkanViewport":
            if (key === "stream_source")
                return { ...kind, stream_source: str() };
            return kind;
        case "SignalTrace":
            if (key === "stream_source")
                return { ...kind, stream_source: str() };
            if (key === "color")
                return { ...kind, color_token: str() };
            return kind;
        case "Label":
            if (key === "text")
                return { ...kind, text_key: str() };
            if (key === "color")
                return { ...kind, color_token: str() };
            return kind;
        case "Clock":
            if (key === "format")
                return { ...kind, format: str() };
            return kind;
        case "NumericDisplay":
            if (key === "requirement")
                return { ...kind, requirement_id: str() };
            if (key === "template")
                return { ...kind, template_id: str() };
            if (key === "source")
                return { ...kind, source: str() };
            if (key === "color")
                return { ...kind, color_token: str() };
            return kind;
        case "StatusIndicator":
            if (key === "requirement")
                return { ...kind, requirement_id: str() };
            if (key === "source")
                return { ...kind, source: str() };
            if (key === "states" && Array.isArray(value))
                return { ...kind, state_text_keys: value };
            if (key === "colors" && Array.isArray(value))
                return { ...kind, color_tokens: value };
            return kind;
        case "Panel":
            return kind;
        case "Image":
            if (key === "source")
                return { ...kind, image_id: str() };
            return kind;
        case "Button":
            if (key === "requirement")
                return { ...kind, requirement_id: optStr() };
            if (key === "label")
                return { ...kind, label_text_key: str() };
            if (key === "color")
                return { ...kind, color_token: str() };
            if (key === "source")
                return { ...kind, source: str() };
            return kind;
        case "TextInput":
            if (key === "requirement")
                return { ...kind, requirement_id: optStr() };
            if (key === "source")
                return { ...kind, source: str() };
            if (key === "max_length")
                return { ...kind, max_length: Number(value) };
            if (key === "charset")
                return { ...kind, glyph_set_id: str() };
            if (key === "color")
                return { ...kind, color_token: str() };
            return kind;
    }
}
