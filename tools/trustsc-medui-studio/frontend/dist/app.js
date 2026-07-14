// Read-only MedUI Studio previewer (ADR-022 wave S9): a screen list and a screen view (pixel-
// exact frame, locale switcher, node-bounds overlay, diagnostics panel, PNG download). No
// editing in this wave — `renderOverlay`/`boundsToStyle` (overlay.ts) are the pieces wave S11's
// editor is expected to build drag/resize on top of.
import { ApiError, frameUrl, listScreens, palette, screenDetail, } from "./api.js";
import { renderOverlay, tooltipText } from "./overlay.js";
const appEl = document.getElementById("app");
if (!appEl) {
    throw new Error("missing #app container");
}
const app = appEl;
let zoom = "fit";
let showGoldenOutlines = false;
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
async function renderScreenList() {
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
function diagnosticsPanel(diagnostics) {
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
async function renderScreenView(screenId, requestedLocale) {
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
    const hoverLabel = el("div", { class: "hover-label" }, [" "]);
    applyZoomStyle(stage, surfaceWidth);
    zoomSelect.addEventListener("change", () => {
        zoom = zoomSelect.value;
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
    children.push(el("div", { class: "frame-viewport" }, [stage]), hoverLabel, diagnosticsPanel(detail.compiled.diagnostics));
    app.replaceChildren(...children);
}
function setHoverLabel(label, node) {
    label.textContent = node ? tooltipText(node).replace(/\n/g, " — ") : " ";
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
