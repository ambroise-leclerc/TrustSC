// Read-only MedUI Studio previewer (ADR-022 wave S9): a screen list and a screen view (pixel-
// exact frame, locale switcher, node-bounds overlay, diagnostics panel, PNG download). No
// editing in this wave — `renderOverlay`/`boundsToStyle` (overlay.ts) are the pieces wave S11's
// editor is expected to build drag/resize on top of.

import {
  ApiError,
  frameUrl,
  listScreens,
  palette,
  screenDetail,
  type CompiledNodeSummary,
  type Diagnostic,
  type ScreenEntry,
} from "./api.js";
import { renderOverlay, tooltipText } from "./overlay.js";

type Zoom = "fit" | "100" | "200";

interface Route {
  screenId: string | null;
  locale: string | null;
}

const appEl = document.getElementById("app");
if (!appEl) {
  throw new Error("missing #app container");
}
const app: HTMLElement = appEl;

let zoom: Zoom = "fit";
let showGoldenOutlines = false;

function parseRoute(): Route {
  const params = new URLSearchParams(location.hash.replace(/^#/, ""));
  return { screenId: params.get("screen"), locale: params.get("locale") };
}

function navigate(screenId: string | null, locale: string | null): void {
  if (screenId === null) {
    location.hash = "";
    return;
  }
  const params = new URLSearchParams({ screen: screenId });
  if (locale) {
    params.set("locale", locale);
  }
  location.hash = params.toString();
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, string> = {},
  children: (Node | string)[] = [],
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    node.setAttribute(key, value);
  }
  for (const child of children) {
    node.append(child);
  }
  return node;
}

async function renderScreenList(): Promise<void> {
  app.replaceChildren(el("p", { class: "status" }, ["Loading screens…"]));
  let screens: ScreenEntry[];
  try {
    screens = await listScreens();
  } catch (error) {
    app.replaceChildren(errorPanel(error));
    return;
  }

  const list = el("ul", { class: "screen-list" });
  for (const screen of screens) {
    const link = el("a", { href: `#screen=${encodeURIComponent(screen.id)}` }, [screen.screen_name]);
    const path = el("span", { class: "screen-list__path" }, [screen.path]);
    list.append(el("li", {}, [link, path]));
  }

  app.replaceChildren(
    el("h1", {}, ["TrustSC MedUI Studio"]),
    el("p", { class: "status" }, [`${screens.length} screen(s) in this repo checkout.`]),
    list,
  );
}

function errorPanel(error: unknown): HTMLElement {
  const message = error instanceof ApiError ? `${error.status}: ${error.message}` : String(error);
  return el("p", { class: "status status--error" }, [message]);
}

function diagnosticsPanel(diagnostics: Diagnostic[]): HTMLElement {
  if (diagnostics.length === 0) {
    return el("div", { class: "diagnostics diagnostics--empty" }, ["No diagnostics."]);
  }
  const items = diagnostics.map((diagnostic) => {
    const location = diagnostic.line !== null ? `line ${diagnostic.line}: ` : "";
    return el("li", {}, [`${location}${diagnostic.message}`]);
  });
  return el("div", { class: "diagnostics diagnostics--error" }, [
    el("strong", {}, [`${diagnostics.length} diagnostic(s)`]),
    el("ul", {}, items),
  ]);
}

function applyZoomStyle(stage: HTMLElement, surfaceWidth: number): void {
  stage.classList.remove("frame-stage--fit", "frame-stage--100", "frame-stage--200");
  stage.classList.add(`frame-stage--${zoom}`);
  if (zoom === "fit") {
    stage.style.width = "";
  } else {
    const multiplier = zoom === "200" ? 2 : 1;
    stage.style.width = `${surfaceWidth * multiplier}px`;
  }
}

async function renderScreenView(screenId: string, requestedLocale: string | null): Promise<void> {
  app.replaceChildren(el("p", { class: "status" }, [`Loading ${screenId}…`]));

  let detail;
  let paletteData;
  try {
    [detail, paletteData] = await Promise.all([screenDetail(screenId), palette()]);
  } catch (error) {
    app.replaceChildren(errorPanel(error));
    return;
  }

  const locale = requestedLocale ?? paletteData.locales[0] ?? "en-US";
  const [surfaceWidth, surfaceHeight] = detail.compiled.surface;

  const backLink = el("a", { href: "#", class: "back-link" }, ["← all screens"]);

  const localeSelect = el("select", { class: "locale-select" });
  for (const candidate of paletteData.locales) {
    const option = el("option", { value: candidate }, [candidate]);
    if (candidate === locale) {
      option.setAttribute("selected", "selected");
    }
    localeSelect.append(option);
  }
  localeSelect.addEventListener("change", () => navigate(screenId, localeSelect.value));

  const zoomSelect = el("select", { class: "zoom-select" });
  for (const [value, label] of [
    ["fit", "Fit"],
    ["100", "100%"],
    ["200", "200%"],
  ] as const) {
    const option = el("option", { value }, [label]);
    if (value === zoom) {
      option.setAttribute("selected", "selected");
    }
    zoomSelect.append(option);
  }

  const goldenToggle = el("label", { class: "golden-toggle" }, [
    el("input", { type: "checkbox", ...(showGoldenOutlines ? { checked: "checked" } : {}) }),
    " golden-reference outlines",
  ]);
  const goldenCheckbox = goldenToggle.querySelector("input");

  const downloadLink = el(
    "a",
    { href: frameUrl(screenId, locale), download: `${screenId.replace(/[/\\]/g, "_")}.png`, class: "download-link" },
    ["Download PNG"],
  );

  const img = el("img", {
    src: frameUrl(screenId, locale),
    width: String(surfaceWidth),
    height: String(surfaceHeight),
    alt: screenId,
    class: "frame-image",
  });
  const stage = el("div", { class: "frame-stage" }, [img]);
  const hoverLabel = el("div", { class: "hover-label" }, [" "]);

  applyZoomStyle(stage, surfaceWidth);
  zoomSelect.addEventListener("change", () => {
    zoom = zoomSelect.value as Zoom;
    applyZoomStyle(stage, surfaceWidth);
  });
  goldenCheckbox?.addEventListener("change", () => {
    showGoldenOutlines = goldenCheckbox.checked;
    renderOverlay(stage, detail.compiled.nodes, surfaceWidth, surfaceHeight, {
      showGoldenOutlines,
      onHover: (node) => setHoverLabel(hoverLabel, node),
    });
  });

  img.addEventListener("load", () => {
    renderOverlay(stage, detail.compiled.nodes, surfaceWidth, surfaceHeight, {
      showGoldenOutlines,
      onHover: (node) => setHoverLabel(hoverLabel, node),
    });
  });

  app.replaceChildren(
    el("div", { class: "toolbar" }, [
      backLink,
      el("h1", {}, [screenId]),
      el("div", { class: "toolbar__spacer" }),
      el("label", {}, ["Locale: ", localeSelect]),
      el("label", {}, ["Zoom: ", zoomSelect]),
      goldenToggle,
      downloadLink,
    ]),
    el("div", { class: "frame-viewport" }, [stage]),
    hoverLabel,
    diagnosticsPanel(detail.compiled.diagnostics),
  );
}

function setHoverLabel(label: HTMLElement, node: CompiledNodeSummary | null): void {
  label.textContent = node ? tooltipText(node).replace(/\n/g, " — ") : " ";
}

async function render(): Promise<void> {
  const route = parseRoute();
  if (route.screenId) {
    await renderScreenView(route.screenId, route.locale);
  } else {
    await renderScreenList();
  }
}

window.addEventListener("hashchange", () => {
  void render();
});
void render();
