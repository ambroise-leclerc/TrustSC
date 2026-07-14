// Typed fetch wrappers over the studio REST API (ADR-022 wave S8). Kept in sync by hand with
// tools/trustsc-medui-studio/src/dto.rs — there is no shared schema generator (yet); if a field
// here stops matching the server, every call site fails loudly (missing/undefined field) rather
// than silently, since this file has no runtime validation of its own.

export interface ScreenEntry {
  id: string;
  path: string;
  screen_name: string;
}

export interface Bounds {
  x: number;
  y: number;
  w: number;
  h: number;
}

// The "kind" tag plus whatever fields that widget kind carries (see NodeKindDto in dto.rs). The
// previewer only ever reads `kind` itself for the tooltip/badge; other fields are looked up by
// name when present (e.g. `text_key`) rather than fully typed, since a read-only previewer has
// no need to round-trip this shape the way the (future, S11) editor will.
export interface NodeKindDto {
  kind: string;
  [field: string]: unknown;
}

export type CvCheckKind = "Bounds" | "ColorHash";

export interface CompiledNodeSummary {
  id: string;
  kind: NodeKindDto;
  bounds: Bounds;
  safety_critical: boolean;
  golden_checks: CvCheckKind[];
}

export type Severity = "Error";

export interface Diagnostic {
  message: string;
  line: number | null;
  severity: Severity;
}

export interface CompiledSummary {
  surface: [number, number];
  nodes: CompiledNodeSummary[];
  diagnostics: Diagnostic[];
}

// The AST DTO (ScreenDefinitionDto in dto.rs) is opaque here: the read-only previewer never
// edits it, only round-trips it through /api/serialize in a later wave. Typed as `unknown` on
// purpose so this file doesn't silently drift from the editor's eventual, much larger AST types.
export type ScreenAst = unknown;

export interface ScreenDetail {
  source: string;
  screen: ScreenAst | null;
  compiled: CompiledSummary;
}

export interface ColorSwatch {
  token: string;
  rgba: [number, number, number, number];
}

export interface LocaleEntry {
  locale: string;
  value: string;
  width_px: number;
  height_px: number;
}

export interface TextKeyInfo {
  string_id: string;
  entries: LocaleEntry[];
}

export interface Palette {
  widgets: unknown[];
  colors: ColorSwatch[];
  text_keys: TextKeyInfo[];
  templates: unknown[];
  images: { id: string; width: number; height: number }[];
  locales: string[];
}

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(path, { headers: { accept: "application/json" } });
  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`;
    try {
      const body = (await response.json()) as { error?: string };
      if (body.error) {
        message = body.error;
      }
    } catch {
      // Non-JSON error body (e.g. a 401 from the auth middleware) — the status text is enough.
    }
    throw new ApiError(response.status, message);
  }
  return (await response.json()) as T;
}

export function listScreens(): Promise<ScreenEntry[]> {
  return getJson<ScreenEntry[]>("/api/screens");
}

export function screenDetail(id: string): Promise<ScreenDetail> {
  return getJson<ScreenDetail>(`/api/screens/${id.split("/").map(encodeURIComponent).join("/")}`);
}

export function palette(): Promise<Palette> {
  return getJson<Palette>("/api/palette");
}

/** The `/api/frame` URL for an `<img src>` — not fetched directly, since the browser handles
 * PNG loading (and caching) itself. */
export function frameUrl(screenId: string, locale: string): string {
  const params = new URLSearchParams({ screen: screenId, locale });
  return `/api/frame?${params.toString()}`;
}
