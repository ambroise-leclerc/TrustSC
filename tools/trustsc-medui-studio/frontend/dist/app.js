// MedUI Studio previewer + canvas editor (ADR-022 waves S9/S11): a screen list and a screen view
// (pixel-exact frame, locale switcher, node-bounds overlay, diagnostics panel, PNG download,
// and — wave S11 — canvas selection/drag/resize with a debounced compile loop).
import { ApiError, frameUrl, listScreens, palette, screenDetail, } from "./api.js";
import { renderOverlay, tooltipText } from "./overlay.js";
import { CanvasEditor, WIDGET_DRAG_MIME } from "./editor.js";
import { isDraggable } from "./ast.js";
import { changeCount, describeChange, hasGoldenImpact } from "./changes.js";
import { el } from "./dom.js";
import { Inspector } from "./inspector.js";
import { cannotCreateReason } from "./palette-defaults.js";
import { openProposeDialog } from "./propose.js";
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
/** The wave-S12 palette panel: one draggable entry per governed widget kind, with the catalog
 * description as tooltip. Kinds whose governed sets are empty in this repo (no baked images, no
 * approved text keys, ...) render disabled with the reason, instead of allowing a drop that could
 * only fail. */
function buildPalettePanel(paletteData) {
    const list = el("ul", { class: "palette__list" });
    for (const widget of paletteData.widgets) {
        const reason = cannotCreateReason(widget.kind_name, paletteData);
        const item = el("li", { class: "palette__item" }, [widget.kind_name]);
        if (reason) {
            item.classList.add("palette__item--disabled");
            item.title = `${widget.description}\nUnavailable: ${reason}`;
        }
        else {
            item.title = widget.description;
            item.setAttribute("draggable", "true");
            item.addEventListener("dragstart", (event) => {
                if (!event.dataTransfer) {
                    return;
                }
                event.dataTransfer.setData(WIDGET_DRAG_MIME, widget.kind_name);
                event.dataTransfer.effectAllowed = "copy";
            });
        }
        list.append(item);
    }
    return el("aside", { class: "palette" }, [
        el("h2", { class: "palette__title" }, ["Palette"]),
        el("p", { class: "palette__hint" }, ["Drag a widget onto the canvas."]),
        list,
    ]);
}
/** Wave S14: the golden-impact warning banner and the changes-summary drawer, re-rendered from
 * the diff the editor reports after every commit and undo/redo. Wave S15's proposal dialog
 * reuses `describeChange`/`changeCount` for the same entries. */
function renderChanges(banner, drawer, list, summary, proposeButton, diff) {
    banner.hidden = !hasGoldenImpact(diff);
    const count = changeCount(diff);
    drawer.hidden = count === 0;
    summary.textContent = `Changes vs. loaded file (${count})`;
    const items = diff.entries.map((entry) => el("li", entry.goldenAffected ? { class: "changes-drawer__golden" } : {}, [describeChange(entry)]));
    if (diff.screenChanged) {
        items.push(el("li", {}, ["screen layout/surface changed"]));
    }
    list.replaceChildren(...items);
    proposeButton.disabled = count === 0;
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
    // With an editable document (wave S13's inspector edits included), a locale switch must not
    // re-navigate — that would discard every in-progress canvas edit. The editor re-renders its
    // in-memory document in the new locale instead, and the URL hash is updated in place so the
    // shareable-URL property is preserved. The read-only fallback keeps the old navigate behavior.
    localeSelect.addEventListener("change", () => {
        if (currentEditor) {
            const params = new URLSearchParams({ screen: screenId, locale: localeSelect.value });
            history.replaceState(null, "", `#${params.toString()}`);
            downloadLink.setAttribute("href", frameUrl(screenId, localeSelect.value));
            currentEditor.setLocale(localeSelect.value);
        }
        else {
            navigate(screenId, localeSelect.value);
        }
    });
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
    let inspector = null;
    const inspectorPanel = el("aside", { class: "inspector" });
    const safetyBanner = el("div", { class: "safety-banner" }, [
        "\u{1F6E1} Golden references / lavapipe ColorHash baselines change — CI re-approval required.",
    ]);
    safetyBanner.hidden = true;
    const changesSummary = el("summary", {}, ["Changes vs. loaded file (0)"]);
    const changesList = el("ul", { class: "changes-drawer__list" });
    const changesDrawer = el("details", { class: "changes-drawer" }, [changesSummary, changesList]);
    changesDrawer.hidden = true;
    // Wave S15: disabled until there's something to propose, and while the document doesn't
    // compile (a proposal the server would only reject anyway).
    const proposeButton = el("button", { type: "button", class: "propose-button", disabled: "disabled" }, [
        "Propose change…",
    ]);
    if (detail.screen) {
        // Editable: the AST DTO is the document the CanvasEditor mutates in memory. It owns the
        // overlay entirely from here on (selection, drag/resize, flow badges, the compile loop) —
        // renderOverlay/boundsToStyle (overlay.ts) stay the shared geometry primitives underneath.
        const editor = new CanvasEditor(locale, stage, img, detail.screen, detail.compiled, paletteData, {
            onHover: (node) => setHoverLabel(hoverLabel, node),
            onDiagnostics: (diagnostics) => {
                renderDiagnostics(diagnosticsContainer, diagnostics);
                proposeButton.disabled = diagnostics.length > 0 || !!changesDrawer.hidden;
            },
            onSelectionChange: (node) => {
                selectionLabel.textContent = node ? selectionStatusText(node) : " ";
                inspector?.showNode(node?.id ?? null);
            },
            onRowSelected: (rowId) => {
                selectionLabel.textContent = `Selected row: ${rowId}`;
                inspector?.showRow(rowId);
            },
            onCompileError: (message) => {
                renderDiagnostics(diagnosticsContainer, [{ message, line: null, severity: "Error" }]);
                proposeButton.disabled = true;
            },
            onDocumentChanged: (diff) => renderChanges(safetyBanner, changesDrawer, changesList, changesSummary, proposeButton, diff),
        });
        currentEditor = editor;
        inspector = new Inspector(inspectorPanel, paletteData, editor);
        if (showGoldenOutlines) {
            editor.setShowGoldenOutlines(true);
        }
        goldenCheckbox?.addEventListener("change", () => {
            showGoldenOutlines = goldenCheckbox.checked;
            editor.setShowGoldenOutlines(showGoldenOutlines);
        });
        proposeButton.addEventListener("click", () => {
            openProposeDialog({ screenId, editor, baseSourceSha256: detail.source_sha256 });
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
            ...(detail.screen ? [proposeButton] : []),
        ]),
    ];
    if (localeWarning) {
        children.push(el("p", { class: "status status--error" }, [localeWarning]));
    }
    const frameViewport = el("div", { class: "frame-viewport" }, [stage]);
    // The palette and inspector only make sense when there is an editable AST.
    const canvasArea = detail.screen
        ? el("div", { class: "editor-layout" }, [buildPalettePanel(paletteData), frameViewport, inspectorPanel])
        : frameViewport;
    children.push(safetyBanner, canvasArea, hoverLabel, selectionLabel, diagnosticsContainer, changesDrawer);
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
