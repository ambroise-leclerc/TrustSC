// Typed fetch wrappers over the studio REST API (ADR-022 wave S8). Kept in sync by hand with
// tools/trustsc-medui-studio/src/dto.rs — there is no shared schema generator (yet); if a field
// here stops matching the server, every call site fails loudly (missing/undefined field) rather
// than silently, since this file has no runtime validation of its own.
export class ApiError extends Error {
    constructor(status, message) {
        super(message);
        this.status = status;
        this.name = "ApiError";
    }
}
async function getJson(path) {
    const response = await fetch(path, { headers: { accept: "application/json" } });
    if (!response.ok) {
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
        throw new ApiError(response.status, message);
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
 * PNG loading (and caching) itself. */
export function frameUrl(screenId, locale) {
    const params = new URLSearchParams({ screen: screenId, locale });
    return `/api/frame?${params.toString()}`;
}
