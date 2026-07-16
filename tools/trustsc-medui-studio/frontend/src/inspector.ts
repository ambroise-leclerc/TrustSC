// Property inspector (ADR-022 wave S13): a panel bound to the canvas selection whose editors are
// generated from the governed widget catalog's PropSchema domains — a dropdown wherever the DSL
// has a closed set (color tokens, approved text keys, baked images, templates, system events,
// clock formats, charsets), a validated field where it doesn't (identifiers, requirement ids,
// sources, px dimensions). Every applied change flows through the editor's wave-S11 compile loop;
// this file never touches the AST document directly.

import type { CvCheckKind, DimensionDto, NodeDefinitionDto, Palette, PropSchema, TextKeyInfo } from "./api.js";
import { collectIds, dimensionPx, findNode, findRow, isValidIdentifier } from "./ast.js";
import { el } from "./dom.js";
import type { CanvasEditor } from "./editor.js";
import { getKindProp, withKindProp } from "./kind-props.js";

/** The compiler's own fallback token for StatusIndicator states declared without `colors:` —
 * used to pad a color list when states are added in the inspector. */
const NEUTRAL_COLOR_TOKEN = "Theme.Colors.Neutral";

type Target = { kind: "none" } | { kind: "node"; id: string } | { kind: "row"; id: string };

export class Inspector {
  private target: Target = { kind: "none" };

  constructor(
    private readonly container: HTMLElement,
    private readonly palette: Palette,
    private readonly editor: CanvasEditor,
  ) {
    this.renderGlobal();
  }

  showNode(nodeId: string | null): void {
    if (nodeId === null) {
      this.target = { kind: "none" };
      this.renderGlobal();
      return;
    }
    // A re-selection of the same node while an inspector control has focus is that control's own
    // edit echoing back — re-rendering now would destroy the element mid-interaction.
    if (
      this.target.kind === "node" &&
      this.target.id === nodeId &&
      this.container.contains(document.activeElement)
    ) {
      return;
    }
    this.target = { kind: "node", id: nodeId };
    this.renderNode(nodeId);
  }

  showRow(rowId: string): void {
    this.target = { kind: "row", id: rowId };
    this.renderRow(rowId);
  }

  // -------------------------------------------------------------------------------------------
  // Views
  // -------------------------------------------------------------------------------------------

  private renderGlobal(): void {
    const screen = this.editor.getScreen();
    const children: (Node | string)[] = [
      el("h2", { class: "inspector__title" }, ["Screen"]),
      el("p", { class: "inspector__hint" }, [
        "Click a node (or a row background) on the canvas to edit its properties.",
      ]),
      this.field(
        "Layout spacing (px)",
        this.numberInput(screen.layout.spacing, 0, (value) =>
          this.editor.applyScreenUpdate((s) => ({ ...s, layout: { ...s.layout, spacing: value } })),
        ),
      ),
      this.field(
        "Layout padding (px)",
        this.numberInput(screen.layout.padding, 0, (value) =>
          this.editor.applyScreenUpdate((s) => ({ ...s, layout: { ...s.layout, padding: value } })),
        ),
      ),
      this.surfaceField(screen.declared_surface),
    ];
    this.container.replaceChildren(...children);
  }

  private renderNode(nodeId: string): void {
    const node = findNode(this.editor.getScreen(), nodeId);
    if (!node) {
      this.target = { kind: "none" };
      this.renderGlobal();
      return;
    }
    const schema = this.palette.widgets.find((w) => w.kind_name === node.kind.kind);
    const children: (Node | string)[] = [
      el("h2", { class: "inspector__title" }, [node.kind.kind]),
    ];
    if (schema) {
      children.push(el("p", { class: "inspector__hint" }, [schema.description]));
    }
    children.push(
      this.idField(node.id, (newId) => {
        this.editor.applyNodeUpdate(nodeId, (n) => ({ ...n, id: newId }));
        this.target = { kind: "node", id: newId };
        this.renderNode(newId);
      }),
    );

    const positioned = node.position !== null;
    for (const dimKey of ["width", "height"] as const) {
      const prop = schema?.properties.find((p) => p.key === dimKey);
      const fillAllowed = prop?.domain.kind === "DimensionPx" ? prop.domain.fill_allowed : true;
      children.push(
        this.field(
          dimKey === "width" ? "Width" : "Height",
          this.dimensionInput(node[dimKey], fillAllowed, positioned, (dim) =>
            this.editor.applyNodeUpdate(this.targetNodeId(), (n) => ({ ...n, [dimKey]: dim })),
          ),
        ),
      );
    }

    if (node.position !== null) {
      children.push(this.positionField(node.position));
    } else {
      children.push(
        el("p", { class: "inspector__hint" }, [
          "Flow node — right-click it on the canvas for “Convert to absolute”.",
        ]),
      );
    }

    if (schema) {
      children.push(...this.kindPropFields(node, schema));
      if (schema.safety_critical_eligible) {
        children.push(this.safetyCriticalField(node));
      }
    }
    this.container.replaceChildren(...children);
  }

  /** Wave S14 guard rail: the `@safety_critical(cv_check: [...])` annotation. An annotation with
   * an empty check list is a compile error, so unchecking the last check is rejected at the
   * field (inline error, AST untouched) instead of diagnosed after the fact. */
  private safetyCriticalField(node: NodeDefinitionDto): HTMLElement {
    const error = el("span", { class: "inspector__error" });
    const nodeId = this.targetNodeId();
    const applyChecks = (checks: CvCheckKind[] | null): void => {
      this.editor.applyNodeUpdate(nodeId, (n) => ({
        ...n,
        safety_critical: checks === null ? null : { cv_checks: checks },
      }));
      this.renderNode(nodeId);
    };

    const master = el("input", {
      type: "checkbox",
      ...(node.safety_critical ? { checked: "checked" } : {}),
    });
    master.addEventListener("change", () => {
      applyChecks(master.checked ? ["Bounds"] : null);
    });
    const masterLabel = el("label", { class: "inspector__fill" }, [master, " @safety_critical"]);

    const rows: (Node | string)[] = [masterLabel];
    if (node.safety_critical) {
      const current = node.safety_critical.cv_checks;
      for (const check of ["Bounds", "ColorHash"] as const) {
        const box = el("input", {
          type: "checkbox",
          ...(current.includes(check) ? { checked: "checked" } : {}),
        });
        // The compiler rejects ColorHash on Image (only Bounds is eligible there).
        const colorHashOnImage = check === "ColorHash" && node.kind.kind === "Image";
        if (colorHashOnImage) {
          box.setAttribute("disabled", "disabled");
        }
        box.addEventListener("change", () => {
          const updated = box.checked ? [...current, check] : current.filter((c) => c !== check);
          // Normalize to a fixed order (matching the compiler's own Bounds-first convention,
          // e.g. `golden_references_push`): otherwise unchecking then rechecking a box reorders
          // the array without changing its meaning, which would misfire as an annotation change
          // in the wave-S14 diff (goldenAffected) for a semantically no-op edit.
          const next = (["Bounds", "ColorHash"] as const).filter((candidate) => updated.includes(candidate));
          if (next.length === 0) {
            error.textContent = "at least one cv_check is required (a compile error otherwise)";
            box.checked = true;
            return;
          }
          error.textContent = "";
          applyChecks(next);
        });
        rows.push(
          el("label", { class: "inspector__fill", ...(colorHashOnImage ? { title: "ColorHash is not eligible on Image" } : {}) }, [
            box,
            ` ${check}`,
          ]),
        );
      }
      rows.push(error);
    }

    return el("div", { class: "inspector__field" }, [
      el("label", { class: "inspector__label" }, ["safety critical"]),
      el("span", { class: "inspector__pair inspector__cv-checks" }, rows),
    ]);
  }

  private renderRow(rowId: string): void {
    const row = findRow(this.editor.getScreen(), rowId);
    if (!row) {
      this.target = { kind: "none" };
      this.renderGlobal();
      return;
    }
    const children: (Node | string)[] = [
      el("h2", { class: "inspector__title" }, ["Row"]),
      this.idField(row.id, (newId) => {
        this.editor.applyRowUpdate(rowId, (r) => ({ ...r, id: newId }));
        this.target = { kind: "row", id: newId };
        this.renderRow(newId);
      }),
      this.field(
        "Height",
        this.dimensionInput(row.height, true, false, (dim) =>
          this.editor.applyRowUpdate(this.targetRowId(), (r) => ({ ...r, height: dim })),
        ),
      ),
      this.field(
        "Spacing (px)",
        this.numberInput(row.spacing, 0, (value) =>
          this.editor.applyRowUpdate(this.targetRowId(), (r) => ({ ...r, spacing: value })),
        ),
      ),
      this.field(
        "Background",
        this.colorSelect(row.background, true, (token) =>
          this.editor.applyRowUpdate(this.targetRowId(), (r) => ({ ...r, background: token })),
        ),
      ),
    ];
    this.container.replaceChildren(...children);
  }

  /** The kind-specific property editors, generated from the widget schema's domains. `id`,
   * `width`, `height`, and `position` live on the node itself and are rendered separately;
   * StatusIndicator's `states`/`colors` pair is edited as one list (the compiler requires equal
   * lengths, so editing them independently could only create broken intermediate states). */
  private kindPropFields(node: NodeDefinitionDto, schema: { properties: PropSchema[] }): HTMLElement[] {
    const fields: HTMLElement[] = [];
    for (const prop of schema.properties) {
      if (["id", "width", "height", "position"].includes(prop.key)) {
        continue;
      }
      const current = getKindProp(node.kind, prop.key);
      const apply = (value: string | number | string[] | null): void =>
        this.editor.applyNodeUpdate(this.targetNodeId(), (n) => ({
          ...n,
          kind: withKindProp(n.kind, prop.key, value),
        }));

      switch (prop.domain.kind) {
        case "ColorToken":
          fields.push(this.field(prop.key, this.colorSelect(String(current ?? ""), false, apply)));
          break;
        case "TextKey":
          fields.push(this.textKeyField(prop.key, String(current ?? ""), dimensionPx(node.width), apply));
          break;
        case "TextKeyList":
          fields.push(this.statesField(node));
          break;
        case "ColorTokenList":
          break; // edited together with the states list above
        case "ImageRef":
          fields.push(this.field(prop.key, this.imageSelect(String(current ?? ""))));
          break;
        case "TemplateId":
          fields.push(this.field(prop.key, this.templateSelect(String(current ?? ""), apply)));
          break;
        case "SystemEvent":
          fields.push(this.field(prop.key, this.enumSelect(["NoOp", "TriggerHalt"], String(current ?? ""), apply)));
          break;
        case "ClockFormat":
          fields.push(
            this.field(prop.key, this.enumSelect(["TimeSeconds", "DateTimeSeconds"], String(current ?? ""), apply)),
          );
          break;
        case "Charset":
          // One approved charset (AsciiText -> SET-ASCII-TEXT); shown for transparency, not
          // (yet) a real choice.
          fields.push(
            this.field(prop.key, this.enumSelect(["AsciiText"], "AsciiText", () => undefined, true)),
          );
          break;
        case "MaxLength":
          fields.push(this.field(prop.key, this.numberInput(Number(current ?? 1), 1, apply)));
          break;
        case "RequirementId": {
          const input = this.textInput(String(current ?? ""), (value) => apply(value.trim()));
          input.placeholder = prop.domain.optional ? "REQ-… (optional)" : "REQ-…";
          fields.push(this.field(prop.key, input));
          break;
        }
        case "QuotedSource":
        case "StreamSource":
          fields.push(this.field(prop.key, this.textInput(String(current ?? ""), (value) => apply(value.trim()))));
          break;
        default:
          break;
      }
    }
    return fields;
  }

  // -------------------------------------------------------------------------------------------
  // Field builders
  // -------------------------------------------------------------------------------------------

  private field(labelText: string, control: Node): HTMLElement {
    return el("div", { class: "inspector__field" }, [
      el("label", { class: "inspector__label" }, [labelText]),
      control,
    ]);
  }

  private targetNodeId(): string {
    return this.target.kind === "node" ? this.target.id : "";
  }

  private targetRowId(): string {
    return this.target.kind === "row" ? this.target.id : "";
  }

  /** Identifier field with inline validation: the rename is only applied when the new id is a
   * valid identifier and not already taken anywhere in the screen — otherwise the error shows at
   * the field and the AST stays untouched. */
  private idField(currentId: string, onRename: (newId: string) => void): HTMLElement {
    const input = el("input", { type: "text", class: "inspector__control", value: currentId });
    const error = el("span", { class: "inspector__error" });
    input.addEventListener("change", () => {
      const newId = input.value.trim();
      if (newId === currentId) {
        error.textContent = "";
        return;
      }
      if (!isValidIdentifier(newId)) {
        error.textContent = "ids are ASCII letters, digits, _ and - only";
        input.value = currentId;
        return;
      }
      if (collectIds(this.editor.getScreen()).has(newId)) {
        error.textContent = `id "${newId}" is already taken`;
        input.value = currentId;
        return;
      }
      error.textContent = "";
      onRename(newId);
    });
    return el("div", { class: "inspector__field" }, [
      el("label", { class: "inspector__label" }, ["id"]),
      input,
      error,
    ]);
  }

  private numberInput(current: number, min: number, onCommit: (value: number) => void): HTMLElement {
    const input = el("input", {
      type: "number",
      class: "inspector__control",
      min: String(min),
      value: String(current),
    });
    input.addEventListener("change", () => {
      const value = Number(input.value);
      if (Number.isFinite(value) && value >= min) {
        onCommit(Math.round(value));
      } else {
        input.value = String(current);
      }
    });
    return input;
  }

  private textInput(current: string, onCommit: (value: string) => void): HTMLInputElement {
    const input = el("input", { type: "text", class: "inspector__control", value: current });
    input.addEventListener("change", () => onCommit(input.value));
    return input;
  }

  private enumSelect(
    options: string[],
    current: string,
    onPick: (value: string) => void,
    disabled = false,
  ): HTMLElement {
    const select = el("select", { class: "inspector__control" });
    for (const option of options) {
      const opt = el("option", { value: option }, [option]);
      if (option === current) {
        opt.setAttribute("selected", "selected");
      }
      select.append(opt);
    }
    if (disabled) {
      select.setAttribute("disabled", "disabled");
    }
    select.addEventListener("change", () => onPick(select.value));
    return select;
  }

  /** Color-token dropdown with an RGBA swatch that tracks the selection. `allowNone` adds a
   * "(none)" entry mapping to `null` (a Row without a `background:`). */
  private colorSelect(
    current: string | null,
    allowNone: boolean,
    onPick: (token: string | null) => void,
  ): HTMLElement {
    const select = el("select", { class: "inspector__control" });
    if (allowNone) {
      const opt = el("option", { value: "" }, ["(none)"]);
      if (current === null) {
        opt.setAttribute("selected", "selected");
      }
      select.append(opt);
    }
    for (const swatch of this.palette.colors) {
      const opt = el("option", { value: swatch.token }, [swatch.token]);
      if (swatch.token === current) {
        opt.setAttribute("selected", "selected");
      }
      select.append(opt);
    }
    const swatchEl = el("span", { class: "inspector__swatch", "aria-hidden": "true" });
    const updateSwatch = (): void => {
      const picked = this.palette.colors.find((c) => c.token === select.value);
      swatchEl.style.background = picked
        ? `rgba(${picked.rgba
            .slice(0, 3)
            .map((channel) => Math.round(channel * 255))
            .join(", ")}, ${picked.rgba[3]})`
        : "transparent";
    };
    updateSwatch();
    select.addEventListener("change", () => {
      updateSwatch();
      onPick(select.value === "" ? null : select.value);
    });
    return el("span", { class: "inspector__color" }, [swatchEl, select]);
  }

  /** Approved-text-key picker: a filter input over the key dropdown, an over-budget flag (⚠) on
   * every key whose worst-case locale is wider than the node, and a per-locale detail list for
   * the selected key — so a budget overrun is visible *before* the compile diagnostic. */
  private textKeyField(
    label: string,
    current: string,
    nodeWidthPx: number | null,
    onPick: (key: string) => void,
  ): HTMLElement {
    const overBudget = (info: TextKeyInfo): boolean =>
      nodeWidthPx !== null && info.entries.some((entry) => entry.width_px > nodeWidthPx);

    const select = el("select", { class: "inspector__control" });
    const details = el("ul", { class: "inspector__text-details" });
    const filter = el("input", {
      type: "search",
      class: "inspector__control inspector__filter",
      placeholder: "filter keys…",
    });

    const renderDetails = (): void => {
      const info = this.palette.text_keys.find((k) => k.string_id === select.value);
      details.replaceChildren(
        ...(info?.entries ?? []).map((entry) => {
          const over = nodeWidthPx !== null && entry.width_px > nodeWidthPx;
          const li = el("li", over ? { class: "inspector__text-over" } : {}, [
            `${entry.locale}: “${entry.value}” — ${entry.width_px}px${over ? " ⚠ wider than node" : ""}`,
          ]);
          return li;
        }),
      );
    };
    const renderOptions = (): void => {
      const needle = filter.value.trim().toLowerCase();
      select.replaceChildren(
        ...this.palette.text_keys
          .filter((info) => needle === "" || info.string_id.toLowerCase().includes(needle))
          .map((info) => {
            const opt = el("option", { value: info.string_id }, [
              `${info.string_id}${overBudget(info) ? " ⚠" : ""}`,
            ]);
            if (info.string_id === current) {
              opt.setAttribute("selected", "selected");
            }
            return opt;
          }),
      );
      // Keep the current value shown even when the filter excludes it — a filter is a view,
      // never an edit.
      if (select.value !== current && !select.querySelector(`option[value="${CSS.escape(current)}"]`)) {
        const opt = el("option", { value: current }, [current]);
        opt.setAttribute("selected", "selected");
        select.prepend(opt);
      }
      renderDetails();
    };
    renderOptions();
    filter.addEventListener("input", renderOptions);
    select.addEventListener("change", () => {
      current = select.value;
      renderDetails();
      onPick(select.value);
    });

    return el("div", { class: "inspector__field" }, [
      el("label", { class: "inspector__label" }, [label]),
      filter,
      select,
      details,
    ]);
  }

  /** Baked-image picker; picking one also snaps the node to that image's intrinsic size (the
   * compiler requires an exact match), then re-renders so the geometry fields update. */
  private imageSelect(current: string): HTMLElement {
    const select = el("select", { class: "inspector__control" });
    for (const image of this.palette.images) {
      const opt = el("option", { value: image.id }, [`${image.id} (${image.width}×${image.height})`]);
      if (image.id === current) {
        opt.setAttribute("selected", "selected");
      }
      select.append(opt);
    }
    select.addEventListener("change", () => {
      const image = this.palette.images.find((candidate) => candidate.id === select.value);
      if (!image) {
        return;
      }
      const nodeId = this.targetNodeId();
      this.editor.applyNodeUpdate(nodeId, (n) => ({
        ...n,
        kind: withKindProp(n.kind, "source", image.id),
        width: { kind: "Px", value: image.width },
        height: { kind: "Px", value: image.height },
      }));
      this.renderNode(nodeId);
    });
    return select;
  }

  private templateSelect(current: string, onPick: (id: string) => void): HTMLElement {
    const ids = [...new Set(this.palette.templates.map((template) => template.id))];
    return this.enumSelect(ids, current, onPick);
  }

  private positionField(position: [number, number]): HTMLElement {
    const commit = (axis: 0 | 1, value: number): void =>
      this.editor.applyNodeUpdate(this.targetNodeId(), (n) => {
        const next: [number, number] = n.position ? [...n.position] : [0, 0];
        next[axis] = value;
        return { ...n, position: next };
      });
    return el("div", { class: "inspector__field" }, [
      el("label", { class: "inspector__label" }, ["position (x, y)"]),
      el("span", { class: "inspector__pair" }, [
        this.numberInput(position[0], 0, (value) => commit(0, value)),
        this.numberInput(position[1], 0, (value) => commit(1, value)),
      ]),
    ]);
  }

  private dimensionInput(
    dim: DimensionDto,
    fillAllowed: boolean,
    positioned: boolean,
    onCommit: (dim: DimensionDto) => void,
  ): HTMLElement {
    const px = el("input", {
      type: "number",
      class: "inspector__control",
      min: "1",
      value: dim.kind === "Px" ? String(dim.value) : "",
    });
    if (dim.kind === "Fill") {
      px.setAttribute("disabled", "disabled");
    }
    px.addEventListener("change", () => {
      const value = Number(px.value);
      if (Number.isFinite(value) && value >= 1) {
        onCommit({ kind: "Px", value: Math.round(value) });
      }
    });
    const wrapper = el("span", { class: "inspector__pair" }, [px]);
    if (fillAllowed) {
      const checkbox = el("input", { type: "checkbox", ...(dim.kind === "Fill" ? { checked: "checked" } : {}) });
      if (positioned) {
        // The parser requires fixed px dimensions whenever `position:` is present, so Fill is
        // not offerable on a positioned node.
        checkbox.setAttribute("disabled", "disabled");
      }
      const toggle = el("label", { class: "inspector__fill", title: positioned ? "positioned nodes need fixed px dimensions" : "" }, [
        checkbox,
        " Fill",
      ]);
      checkbox.addEventListener("change", () => {
        if (checkbox.checked) {
          px.setAttribute("disabled", "disabled");
          onCommit({ kind: "Fill" });
        } else {
          px.removeAttribute("disabled");
          // The px input is usually blank here (Fill had disabled it) — reflect the committed
          // fallback in the control so the UI and the AST can't disagree.
          const raw = Number(px.value);
          const value = Number.isFinite(raw) && raw >= 1 ? Math.round(raw) : 100;
          px.value = String(value);
          onCommit({ kind: "Px", value });
        }
      });
      wrapper.append(toggle);
    }
    return wrapper;
  }

  private surfaceField(surface: [number, number] | null): HTMLElement {
    const width = el("input", {
      type: "number",
      class: "inspector__control",
      min: "1",
      value: surface ? String(surface[0]) : "",
      placeholder: "width",
    });
    const height = el("input", {
      type: "number",
      class: "inspector__control",
      min: "1",
      value: surface ? String(surface[1]) : "",
      placeholder: "height",
    });
    const commit = (): void => {
      const w = Number(width.value);
      const h = Number(height.value);
      if (Number.isFinite(w) && w >= 1 && Number.isFinite(h) && h >= 1) {
        this.editor.applyScreenUpdate((s) => ({ ...s, declared_surface: [Math.round(w), Math.round(h)] }));
      }
    };
    width.addEventListener("change", commit);
    height.addEventListener("change", commit);
    const clear = el("button", { type: "button", class: "inspector__clear" }, ["clear"]);
    clear.addEventListener("click", () => {
      width.value = "";
      height.value = "";
      this.editor.applyScreenUpdate((s) => ({ ...s, declared_surface: null }));
    });
    return el("div", { class: "inspector__field" }, [
      el("label", { class: "inspector__label" }, ["surface (px)"]),
      el("span", { class: "inspector__pair" }, [width, height, clear]),
    ]);
  }

  /** StatusIndicator's `states`/`colors` pair, edited as one list of (text key, color) rows so
   * both arrays always keep equal lengths — the compiler rejects a mismatch. */
  private statesField(node: NodeDefinitionDto): HTMLElement {
    if (node.kind.kind !== "StatusIndicator") {
      return el("span");
    }
    const states = [...node.kind.state_text_keys];
    const colors = [...node.kind.color_tokens];
    while (colors.length < states.length) {
      colors.push(NEUTRAL_COLOR_TOKEN);
    }

    const applyLists = (nextStates: string[], nextColors: string[]): void => {
      const nodeId = this.targetNodeId();
      this.editor.applyNodeUpdate(nodeId, (n) => ({
        ...n,
        kind: withKindProp(withKindProp(n.kind, "states", nextStates), "colors", nextColors),
      }));
      this.renderNode(nodeId);
    };

    const rows = states.map((stateKey, index) => {
      const keySelect = this.enumSelect(
        this.palette.text_keys.map((info) => info.string_id),
        stateKey,
        (value) => {
          const next = [...states];
          next[index] = value;
          applyLists(next, colors);
        },
      );
      const colorPick = this.colorSelect(colors[index] ?? NEUTRAL_COLOR_TOKEN, false, (token) => {
        const next = [...colors];
        next[index] = token ?? NEUTRAL_COLOR_TOKEN;
        applyLists(states, next);
      });
      const remove = el("button", { type: "button", class: "inspector__clear", title: "remove this state" }, ["×"]);
      remove.addEventListener("click", () => {
        applyLists(
          states.filter((_, i) => i !== index),
          colors.filter((_, i) => i !== index),
        );
      });
      if (states.length === 1) {
        // A StatusIndicator with zero states can't compile; removing the last one is not
        // offered rather than diagnosed after the fact.
        remove.setAttribute("disabled", "disabled");
      }
      return el("li", { class: "inspector__state-row" }, [keySelect, colorPick, remove]);
    });

    const add = el("button", { type: "button", class: "inspector__clear" }, ["+ add state"]);
    add.addEventListener("click", () => {
      const firstKey = this.palette.text_keys[0]?.string_id;
      if (!firstKey) {
        return;
      }
      applyLists([...states, firstKey], [...colors, NEUTRAL_COLOR_TOKEN]);
    });

    return el("div", { class: "inspector__field" }, [
      el("label", { class: "inspector__label" }, ["states / colors"]),
      el("ul", { class: "inspector__states" }, rows),
      add,
    ]);
  }
}
