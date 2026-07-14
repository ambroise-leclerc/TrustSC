// MedUI Studio previewer + canvas editor (ADR-022 waves S9/S11): a screen list and a screen view
// (pixel-exact frame, locale switcher, node-bounds overlay, diagnostics panel, PNG download,
// and — wave S11 — canvas selection/drag/resize with a debounced compile loop).
import { ApiError, frameUrl, listScreens, palette, screenDetail, } from "./api.js";
import { renderOverlay, tooltipText } from "./overlay.js";
import { CanvasEditor } from "./editor.js";
import { isDraggable } from "./ast.js";
const appEl = document.getElementById("app");
if (!appEl) {
    throw new Error("missing #app container");
}
const app = appEl;
let zoom = "fit";
let showGoldenOutlines = false;
let currentEditor = null;
function parseRoute() {
    const params = new URLSearchParams(location.hash.replace(/^#/, ""));
    return { screenId: params.get("screen"), locale: params.get("locale") };
}
function navigate(screenId, locale) {
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
function el(tag, attrs = {}, children = []) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(attrs)) {
        node.setAttribute(key, value);
    }
    for (const child of children) {
        node.append(child);
    }
    return node;
}
/** Every screen view owns document-level listeners (drag/keyboard) via its `CanvasEditor`; this
 * must run before replacing `#app`'s content on every navigation, or listeners from a previous
 * screen view pile up and keep firing against DOM that no longer exists. */
function teardownEditor() {
    currentEditor?.destroy();
    currentEditor = null;
}
async function renderScreenList() {
    teardownEditor();
    app.replaceChildren(el("p", { class: "status" }, ["Loading screens…"]));
    let screens;
    try {
        screens = await listScreens();
    }
    catch (error) {
        app.replaceChildren(errorPanel(error));
        return;
    }
    const list = el("ul", { class: "screen-list" });
    for (const screen of screens) {
        const link = el("a", { href: `#screen=${encodeURIComponent(screen.id)}` }, [screen.screen_name]);
        const path = el("span", { class: "screen-list__path" }, [screen.path]);
        list.append(el("li", {}, [link, path]));
    }
    app.replaceChildren(el("h1", {}, ["TrustSC MedUI Studio"]), el("p", { class: "status" }, [`${screens.length} screen(s) in this repo checkout.`]), list);
}
function errorPanel(error) {
    const message = error instanceof ApiError ? `${error.status}: ${error.message}` : String(error);
    return el("p", { class: "status status--error" }, [message]);
}
function renderDiagnostics(container, diagnostics) {
    if (diagnostics.length === 0) {
        container.className = "diagnostics diagnostics--empty";
        container.replaceChildren("No diagnostics.");
        return;
    }
    const items = diagnostics.map((diagnostic) => {
        const location = diagnostic.line !== null ? `line ${diagnostic.line}: ` : "";
        return el("li", {}, [`${location}${diagnostic.message}`]);
    });
    container.className = "diagnostics diagnostics--error";
    container.replaceChildren(el("strong", {}, [`${diagnostics.length} diagnostic(s)`]), el("ul", {}, items));
}
function applyZoomStyle(stage, surfaceWidth) {
    stage.classList.remove("frame-stage--fit", "frame-stage--100", "frame-stage--200");
    stage.classList.add(`frame-stage--${zoom}`);
    if (zoom === "fit") {
        stage.style.width = "";
    }
    else {
        const multiplier = zoom === "200" ? 2 : 1;
        stage.style.width = `${surfaceWidth * multiplier}px`;
    }
}
function selectionStatusText(node) {
    const editable = isDraggable(node)
        ? "draggable"
        : "flow (right-click for “Convert to absolute”)";
    return `Selected: ${node.id} (${node.kind.kind}, ${editable})`;
}
async function renderScreenView(screenId, requestedLocale) {
    teardownEditor();
    app.replaceChildren(el("p", { class: "status" }, [`Loading ${screenId}…`]));
    let detail;
    let paletteData;
    try {
        [detail, paletteData] = await Promise.all([screenDetail(screenId), palette()]);
    }
    catch (error) {
        app.replaceChildren(errorPanel(error));
        return;
    }
    // A shared/edited URL can name a locale the palette doesn't know about — /api/frame would
    // 400 on it, leaving the <img> broken with no in-page explanation. Clamp to a known locale
    // up front and say so, rather than letting the request fail silently.
    let locale = requestedLocale ?? paletteData.locales[0] ?? "en-US";
    let localeWarning = null;
    if (!paletteData.locales.includes(locale)) {
        const fallback = paletteData.locales[0] ?? "en-US";
        localeWarning = `Unknown locale "${locale}" — showing "${fallback}" instead.`;
        locale = fallback;
    }
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
    // Switching locale re-navigates (a fresh screenDetail load), which discards any in-progress
    // canvas edit — the editor's document is purely in-memory in this wave (no save until S15),
    // so there's nothing to preserve across it anyway.
    localeSelect.addEventListener("change", () => navigate(screenId, localeSelect.value));
    const zoomSelect = el("select", { class: "zoom-select" });
    for (const [value, label] of [
        ["fit", "Fit"],
        ["100", "100%"],
        ["200", "200%"],
    ]) {
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
    const downloadLink = el("a", { href: frameUrl(screenId, locale), download: `${screenId.replace(/[/\\]/g, "_")}.png`, class: "download-link" }, ["Download PNG"]);
    const img = el("img", {
        src: frameUrl(screenId, locale),
        width: String(surfaceWidth),
        height: String(surfaceHeight),
        alt: screenId,
        class: "frame-image",
    });
    const stage = el("div", { class: "frame-stage" }, [img]);
    const hoverLabel = el("div", { class: "hover-label" }, [" "]);
    const selectionLabel = el("div", { class: "selection-label" }, [" "]);
    const diagnosticsContainer = el("div", { class: "diagnostics diagnostics--empty" }, ["No diagnostics."]);
    applyZoomStyle(stage, surfaceWidth);
    zoomSelect.addEventListener("change", () => {
        zoom = zoomSelect.value;
        applyZoomStyle(stage, surfaceWidth);
    });
    if (detail.screen) {
        // Editable: the AST DTO is the document the CanvasEditor mutates in memory. It owns the
        // overlay entirely from here on (selection, drag/resize, flow badges, the compile loop) —
        // renderOverlay/boundsToStyle (overlay.ts) stay the shared geometry primitives underneath.
        const editor = new CanvasEditor(locale, stage, img, detail.screen, detail.compiled, {
            onHover: (node) => setHoverLabel(hoverLabel, node),
            onDiagnostics: (diagnostics) => renderDiagnostics(diagnosticsContainer, diagnostics),
            onSelectionChange: (node) => {
                selectionLabel.textContent = node ? selectionStatusText(node) : " ";
            },
            onRowSelected: (rowId) => {
                selectionLabel.textContent = `Selected row background: ${rowId} (inspector lands in a later wave)`;
            },
            onCompileError: (message) => {
                renderDiagnostics(diagnosticsContainer, [{ message, line: null, severity: "Error" }]);
            },
        });
        currentEditor = editor;
        if (showGoldenOutlines) {
            editor.setShowGoldenOutlines(true);
        }
        goldenCheckbox?.addEventListener("change", () => {
            showGoldenOutlines = goldenCheckbox.checked;
            editor.setShowGoldenOutlines(showGoldenOutlines);
        });
    }
    else {
        // The source failed to even parse — nothing to edit. Fall back to the plain read-only
        // overlay (S9 behavior) so the page still shows whatever compiled data exists (usually
        // none) instead of a blank canvas.
        renderOverlay(stage, detail.compiled.nodes, surfaceWidth, surfaceHeight, {
            showGoldenOutlines,
            onHover: (node) => setHoverLabel(hoverLabel, node),
        });
        goldenCheckbox?.addEventListener("change", () => {
            showGoldenOutlines = goldenCheckbox.checked;
            renderOverlay(stage, detail.compiled.nodes, surfaceWidth, surfaceHeight, {
                showGoldenOutlines,
                onHover: (node) => setHoverLabel(hoverLabel, node),
            });
        });
    }
    renderDiagnostics(diagnosticsContainer, detail.compiled.diagnostics);
    const children = [
        el("div", { class: "toolbar" }, [
            backLink,
            el("h1", {}, [screenId]),
            el("div", { class: "toolbar__spacer" }),
            el("label", {}, ["Locale: ", localeSelect]),
            el("label", {}, ["Zoom: ", zoomSelect]),
            goldenToggle,
            downloadLink,
        ]),
    ];
    if (localeWarning) {
        children.push(el("p", { class: "status status--error" }, [localeWarning]));
    }
    children.push(el("div", { class: "frame-viewport" }, [stage]), hoverLabel, selectionLabel, diagnosticsContainer);
    app.replaceChildren(...children);
}
function setHoverLabel(label, node) {
    label.textContent = node ? tooltipText(node).replace(/\n/g, " — ") : " ";
}
async function render() {
    const route = parseRoute();
    if (route.screenId) {
        await renderScreenView(route.screenId, route.locale);
    }
    else {
        await renderScreenList();
    }
}
window.addEventListener("hashchange", () => {
    void render();
});
void render();
