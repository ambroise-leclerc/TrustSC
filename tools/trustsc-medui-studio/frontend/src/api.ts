// Typed fetch wrappers over the studio REST API (ADR-022 waves S8/S11). Kept in sync by hand
// with tools/trustsc-medui-studio/src/dto.rs — there is no shared schema generator (yet); if a
// field here stops matching the server, every call site fails loudly (missing/undefined field)
// rather than silently, since this file has no runtime validation of its own.

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

export type CvCheckKind = "Bounds" | "ColorHash";
export type SystemEventDto = "NoOp" | "TriggerHalt";
export type ClockFormatDto = "TimeSeconds" | "DateTimeSeconds";

// Mirrors NodeKindDto (dto.rs) exactly, including Panel: a *compiled* node summary can carry one
// (synthesized from a Row's background:), even though the AST editor never authors one directly
// (see ast.ts's isSyntheticPanel/isDraggable).
export type NodeKindDto =
  | { kind: "CriticalButton"; requirement_id: string; label_text_key: string; color_token: string; on_press: SystemEventDto }
  | { kind: "VulkanViewport"; stream_source: string }
  | { kind: "SignalTrace"; stream_source: string; color_token: string }
  | { kind: "Label"; text_key: string; color_token: string }
  | { kind: "Clock"; format: ClockFormatDto }
  | { kind: "NumericDisplay"; requirement_id: string; template_id: string; source: string; color_token: string }
  | { kind: "StatusIndicator"; requirement_id: string; source: string; state_text_keys: string[]; color_tokens: string[] }
  | { kind: "Panel"; color_token: string }
  | { kind: "Image"; image_id: string }
  | { kind: "Button"; label_text_key: string; color_token: string; source: string; requirement_id: string | null }
  | {
      kind: "TextInput";
      source: string;
      max_length: number;
      glyph_set_id: string;
      color_token: string;
      requirement_id: string | null;
    };

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

// ---------------------------------------------------------------------------------------------
// AST DTOs (wave S11): mirrors ScreenDefinitionDto and everything it owns (dto.rs). This is the
// document the canvas editor mutates in memory and round-trips through /api/compile + /api/frame
// — never persisted anywhere in this wave (no save/PR flow until wave S15).
// ---------------------------------------------------------------------------------------------

export type DimensionDto = { kind: "Px"; value: number } | { kind: "Fill" };

export interface LayoutDefinitionDto {
  kind: "Vertical" | "Horizontal";
  spacing: number;
  padding: number;
}

export interface SafetyCriticalDto {
  cv_checks: CvCheckKind[];
}

export interface NodeDefinitionDto {
  id: string;
  width: DimensionDto;
  height: DimensionDto;
  position: [number, number] | null;
  kind: NodeKindDto;
  safety_critical: SafetyCriticalDto | null;
}

export interface RowDefinitionDto {
  id: string;
  height: DimensionDto;
  spacing: number;
  background: string | null;
  children: NodeDefinitionDto[];
}

// Internally tagged on "type" (not "kind" — NodeDefinitionDto already has its own "kind" field;
// see dto.rs's ScreenItemDto doc comment). Each variant's payload is flattened alongside the tag.
export type ScreenItemDto = ({ type: "Component" } & NodeDefinitionDto) | ({ type: "Row" } & RowDefinitionDto);

export interface ScreenDefinitionDto {
  id: string;
  layout: LayoutDefinitionDto;
  declared_surface: [number, number] | null;
  items: ScreenItemDto[];
}

export interface ScreenDetail {
  source: string;
  /** SHA-256 of `source` as read from disk — echoed back as a proposal's optimistic-concurrency
   * base (wave S15). */
  source_sha256: string;
  screen: ScreenDefinitionDto | null;
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

// Mirrors PropDomainDto (dto.rs): the accepted shape of a widget property value, driving which
// control the palette/inspector renders for it (dropdown, px field, free text, ...).
export type PropDomainDto =
  | { kind: "Identifier" }
  | { kind: "DimensionPx"; fill_allowed: boolean }
  | { kind: "Position" }
  | { kind: "TextKey" }
  | { kind: "TextKeyList" }
  | { kind: "ColorToken" }
  | { kind: "ColorTokenList" }
  | { kind: "QuotedSource" }
  | { kind: "StreamSource" }
  | { kind: "TemplateId" }
  | { kind: "ImageRef" }
  | { kind: "SystemEvent" }
  | { kind: "ClockFormat" }
  | { kind: "Charset" }
  | { kind: "MaxLength" }
  | { kind: "RequirementId"; optional: boolean };

export interface PropSchema {
  key: string;
  required: boolean;
  domain: PropDomainDto;
}

export interface WidgetSchema {
  kind_name: string;
  description: string;
  safety_critical_eligible: boolean;
  properties: PropSchema[];
}

export interface NumericTemplateInfo {
  id: string;
  locale: string;
  max_chars: number;
  glyph_set_id: string;
}

export interface ImageInfo {
  id: string;
  width: number;
  height: number;
}

export interface Palette {
  widgets: WidgetSchema[];
  colors: ColorSwatch[];
  text_keys: TextKeyInfo[];
  templates: NumericTemplateInfo[];
  images: ImageInfo[];
  locales: string[];
}

export interface CompileResult {
  ok: boolean;
  compiled: CompiledSummary | null;
  diagnostics: Diagnostic[];
}

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
    /** Machine-readable error code some endpoints attach (wave S15's proposals: "stale_base",
     * "comment_loss", "uncompilable") so callers can branch without parsing prose. */
    public readonly code: string | null = null,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

async function errorFrom(response: Response): Promise<ApiError> {
  let message = `${response.status} ${response.statusText}`;
  let code: string | null = null;
  try {
    const body = (await response.json()) as { error?: string; code?: string };
    if (body.error) {
      message = body.error;
    }
    code = body.code ?? null;
  } catch {
    // Non-JSON error body (e.g. a 401 from the auth middleware) — the status text is enough.
  }
  return new ApiError(response.status, message, code);
}

async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(path, { headers: { accept: "application/json" } });
  if (!response.ok) {
    throw await errorFrom(response);
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
 * PNG loading (and caching) itself. Always renders the *saved* source, never the in-progress
 * edit — see `postFrame` for that. */
export function frameUrl(screenId: string, locale: string): string {
  const params = new URLSearchParams({ screen: screenId, locale });
  return `/api/frame?${params.toString()}`;
}

/** `POST /api/compile` with an in-memory AST — the canvas editor's compile loop. Never throws
 * for a screen that fails to compile (`ok: false` + `diagnostics` instead); only throws for a
 * genuine transport/request-shape failure. */
export async function compileScreen(screen: ScreenDefinitionDto): Promise<CompileResult> {
  const response = await fetch("/api/compile", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ screen }),
  });
  if (!response.ok) {
    throw await errorFrom(response);
  }
  return (await response.json()) as CompileResult;
}

/** `POST /api/frame` with an in-memory AST, returning the rendered PNG as a `Blob` (the caller
 * turns it into an object URL) — the editor's post-compile frame refresh for unsaved edits. */
export async function postFrame(screen: ScreenDefinitionDto, locale: string): Promise<Blob> {
  const response = await fetch("/api/frame", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ screen, locale }),
  });
  if (!response.ok) {
    throw await errorFrom(response);
  }
  return await response.blob();
}

export interface ProposalRequest {
  screenId: string;
  screen: ScreenDefinitionDto;
  baseSourceSha256: string;
  title: string;
  description: string;
  allowCommentLoss?: boolean;
}

export interface ProposalResult {
  branch: string;
  commit: string;
  prUrl: string | null;
  warning: string | null;
}

/** `POST /api/proposals` (wave S15): serializes `screen` server-side and turns it into a pushed
 * branch (+ PR when possible). Throws `ApiError` with `code === "stale_base"` when the file
 * changed upstream since `baseSourceSha256` was read, `"comment_loss"` when the committed file
 * has `//` comments the caller hasn't confirmed dropping, or `"uncompilable"` when the server's
 * own re-compile of `screen` fails — the caller branches on `error.code`, not on parsing prose. */
export async function proposeChange(request: ProposalRequest): Promise<ProposalResult> {
  const response = await fetch("/api/proposals", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      screen_id: request.screenId,
      screen: request.screen,
      base_source_sha256: request.baseSourceSha256,
      title: request.title,
      description: request.description,
      allow_comment_loss: request.allowCommentLoss ?? false,
    }),
  });
  if (!response.ok) {
    throw await errorFrom(response);
  }
  const body = (await response.json()) as { branch: string; commit: string; pr_url: string | null; warning: string | null };
  return { branch: body.branch, commit: body.commit, prUrl: body.pr_url, warning: body.warning };
}
