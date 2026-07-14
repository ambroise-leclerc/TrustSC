// Typed fetch wrappers over the studio REST API (ADR-022 waves S8/S11). Kept in sync by hand
// with tools/trustsc-medui-studio/src/dto.rs — there is no shared schema generator (yet); if a
// field here stops matching the server, every call site fails loudly (missing/undefined field)
// rather than silently, since this file has no runtime validation of its own.
export class ApiError extends Error {
    constructor(status, message) {
        super(message);
        this.status = status;
        this.name = "ApiError";
    }
}
async function errorMessageFrom(response) {
    let message = `${response.status} ${response.statusText}`;
    try {
        const body = (await response.json());
        if (body.error) {
            message = body.error;
        }
    }
    catch {
        // Non-JSON error body (e.g. a 401 from the auth middleware) — the status text is enough.
    }
    return message;
}
async function getJson(path) {
    const response = await fetch(path, { headers: { accept: "application/json" } });
    if (!response.ok) {
        throw new ApiError(response.status, await errorMessageFrom(response));
    }
    return (await response.json());
}
export function listScreens() {
    return getJson("/api/screens");
}
export function screenDetail(id) {
    return getJson(`/api/screens/${id.split("/").map(encodeURIComponent).join("/")}`);
}
export function palette() {
    return getJson("/api/palette");
}
/** The `/api/frame` URL for an `<img src>` — not fetched directly, since the browser handles
 * PNG loading (and caching) itself. Always renders the *saved* source (there is no save/PR flow
 * until wave S15, so this is never the in-progress edit — see `postFrame` for that). */
export function frameUrl(screenId, locale) {
    const params = new URLSearchParams({ screen: screenId, locale });
    return `/api/frame?${params.toString()}`;
}
/** `POST /api/compile` with an in-memory AST — the canvas editor's compile loop. Never throws
 * for a screen that fails to compile (`ok: false` + `diagnostics` instead); only throws for a
 * genuine transport/request-shape failure. */
export async function compileScreen(screen) {
    const response = await fetch("/api/compile", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ screen }),
    });
    if (!response.ok) {
        throw new ApiError(response.status, await errorMessageFrom(response));
    }
    return (await response.json());
}
/** `POST /api/frame` with an in-memory AST, returning the rendered PNG as a `Blob` (the caller
 * turns it into an object URL) — the editor's post-compile frame refresh for unsaved edits. */
export async function postFrame(screen, locale) {
    const response = await fetch("/api/frame", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ screen, locale }),
    });
    if (!response.ok) {
        throw new ApiError(response.status, await errorMessageFrom(response));
    }
    return await response.blob();
}
