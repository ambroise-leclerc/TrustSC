//! Palette enumeration APIs: everything a GUI (MedUI Studio, ADR-022) needs to populate governed
//! dropdowns — widget shapes, approved text keys with per-locale measured widths, numeric
//! templates, and baked images — instead of accepting free-typed values the compiler would later
//! reject far from the editing surface. Colors need no enumeration API here: a GUI reads
//! `trustsc_ui::THEME_COLORS` directly for token names and RGBA swatches.

use trustsc_image_schema::ImagePackage;
use trustsc_text_schema::TextPackage;

/// The accepted shape of a widget property value. Mirrors the parser's own grammar
/// (`parse_dimension`, `parse_position`, `parse_text_key`, …) so a GUI can render the right
/// control (a color-token dropdown, a `t("key")` picker, a plain text field) without duplicating
/// parsing logic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropDomain {
    Identifier,
    DimensionPx { fill_allowed: bool },
    Position,
    TextKey,
    TextKeyList,
    ColorToken,
    ColorTokenList,
    QuotedSource,
    StreamSource,
    TemplateId,
    ImageRef,
    SystemEvent,
    ClockFormat,
    Charset,
    MaxLength,
    RequirementId { optional: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropSchema {
    pub key: &'static str,
    pub required: bool,
    pub domain: PropDomain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WidgetSchema {
    pub kind_name: &'static str,
    pub description: &'static str,
    /// Whether `@safety_critical(cv_check: [...])` may annotate this kind at all. `Image` is
    /// still `true` here — it is `Bounds`-eligible, only `ColorHash` is rejected on it
    /// (`compile_screen`'s Image arm), and this single flag does not model per-check rejection.
    pub safety_critical_eligible: bool,
    pub properties: &'static [PropSchema],
}

const ID_PROP: PropSchema = PropSchema {
    key: "id",
    required: true,
    domain: PropDomain::Identifier,
};
const WIDTH_PROP: PropSchema = PropSchema {
    key: "width",
    required: true,
    domain: PropDomain::DimensionPx { fill_allowed: true },
};
const HEIGHT_PROP: PropSchema = PropSchema {
    key: "height",
    required: true,
    domain: PropDomain::DimensionPx { fill_allowed: true },
};
const POSITION_PROP: PropSchema = PropSchema {
    key: "position",
    required: false,
    domain: PropDomain::Position,
};

static CRITICAL_BUTTON_PROPS: &[PropSchema] = &[
    ID_PROP,
    WIDTH_PROP,
    HEIGHT_PROP,
    POSITION_PROP,
    PropSchema { key: "requirement", required: true, domain: PropDomain::RequirementId { optional: false } },
    PropSchema { key: "label", required: true, domain: PropDomain::TextKey },
    PropSchema { key: "color", required: true, domain: PropDomain::ColorToken },
    PropSchema { key: "on_press", required: true, domain: PropDomain::SystemEvent },
];

static VULKAN_VIEWPORT_PROPS: &[PropSchema] = &[
    ID_PROP,
    WIDTH_PROP,
    HEIGHT_PROP,
    POSITION_PROP,
    PropSchema { key: "stream_source", required: true, domain: PropDomain::StreamSource },
];

static SIGNAL_TRACE_PROPS: &[PropSchema] = &[
    ID_PROP,
    WIDTH_PROP,
    HEIGHT_PROP,
    POSITION_PROP,
    PropSchema { key: "stream_source", required: true, domain: PropDomain::StreamSource },
    PropSchema { key: "color", required: true, domain: PropDomain::ColorToken },
];

static LABEL_PROPS: &[PropSchema] = &[
    ID_PROP,
    WIDTH_PROP,
    HEIGHT_PROP,
    POSITION_PROP,
    PropSchema { key: "text", required: true, domain: PropDomain::TextKey },
    PropSchema { key: "color", required: true, domain: PropDomain::ColorToken },
];

static CLOCK_PROPS: &[PropSchema] = &[
    ID_PROP,
    WIDTH_PROP,
    HEIGHT_PROP,
    POSITION_PROP,
    PropSchema { key: "format", required: true, domain: PropDomain::ClockFormat },
];

static NUMERIC_DISPLAY_PROPS: &[PropSchema] = &[
    ID_PROP,
    WIDTH_PROP,
    HEIGHT_PROP,
    POSITION_PROP,
    PropSchema { key: "requirement", required: true, domain: PropDomain::RequirementId { optional: false } },
    PropSchema { key: "template", required: true, domain: PropDomain::TemplateId },
    PropSchema { key: "source", required: true, domain: PropDomain::QuotedSource },
    PropSchema { key: "color", required: true, domain: PropDomain::ColorToken },
];

static STATUS_INDICATOR_PROPS: &[PropSchema] = &[
    ID_PROP,
    WIDTH_PROP,
    HEIGHT_PROP,
    POSITION_PROP,
    PropSchema { key: "requirement", required: true, domain: PropDomain::RequirementId { optional: false } },
    PropSchema { key: "source", required: true, domain: PropDomain::QuotedSource },
    PropSchema { key: "states", required: true, domain: PropDomain::TextKeyList },
    PropSchema { key: "colors", required: false, domain: PropDomain::ColorTokenList },
];

static IMAGE_PROPS: &[PropSchema] = &[
    ID_PROP,
    WIDTH_PROP,
    HEIGHT_PROP,
    POSITION_PROP,
    PropSchema { key: "source", required: true, domain: PropDomain::ImageRef },
];

static BUTTON_PROPS: &[PropSchema] = &[
    ID_PROP,
    WIDTH_PROP,
    HEIGHT_PROP,
    POSITION_PROP,
    PropSchema { key: "requirement", required: false, domain: PropDomain::RequirementId { optional: true } },
    PropSchema { key: "label", required: true, domain: PropDomain::TextKey },
    PropSchema { key: "color", required: true, domain: PropDomain::ColorToken },
    PropSchema { key: "source", required: true, domain: PropDomain::QuotedSource },
];

static TEXT_INPUT_PROPS: &[PropSchema] = &[
    ID_PROP,
    WIDTH_PROP,
    HEIGHT_PROP,
    POSITION_PROP,
    PropSchema { key: "requirement", required: false, domain: PropDomain::RequirementId { optional: true } },
    PropSchema { key: "source", required: true, domain: PropDomain::QuotedSource },
    PropSchema { key: "max_length", required: true, domain: PropDomain::MaxLength },
    PropSchema { key: "charset", required: false, domain: PropDomain::Charset },
    PropSchema { key: "color", required: true, domain: PropDomain::ColorToken },
];

static WIDGET_CATALOG: &[WidgetSchema] = &[
    WidgetSchema {
        kind_name: "CriticalButton",
        description: "A framework-governed button raising a predefined SystemEvent (ADR-015).",
        safety_critical_eligible: true,
        properties: CRITICAL_BUTTON_PROPS,
    },
    WidgetSchema {
        kind_name: "VulkanViewport",
        description: "A reserved region for direct 3D/spectral imaging output.",
        safety_critical_eligible: true,
        properties: VULKAN_VIEWPORT_PROPS,
    },
    WidgetSchema {
        kind_name: "SignalTrace",
        description: "A scrolling 2D amplitude trace for a single-channel physiological signal (ADR-018).",
        safety_critical_eligible: true,
        properties: SIGNAL_TRACE_PROPS,
    },
    WidgetSchema {
        kind_name: "Label",
        description: "Static approved text with no interaction and no requirement.",
        safety_critical_eligible: true,
        properties: LABEL_PROPS,
    },
    WidgetSchema {
        kind_name: "Clock",
        description: "Wall-clock date/time fed by the platform adapter.",
        safety_critical_eligible: true,
        properties: CLOCK_PROPS,
    },
    WidgetSchema {
        kind_name: "NumericDisplay",
        description: "A live numeric value bound to an approved NumericTemplate and a realtime data source.",
        safety_critical_eligible: true,
        properties: NUMERIC_DISPLAY_PROPS,
    },
    WidgetSchema {
        kind_name: "StatusIndicator",
        description: "An enumerated device-state display selected by index at runtime.",
        safety_critical_eligible: true,
        properties: STATUS_INDICATOR_PROPS,
    },
    WidgetSchema {
        kind_name: "Image",
        description: "A governed raster image rendered at its baked intrinsic size only (ADR-014).",
        safety_critical_eligible: true,
        properties: IMAGE_PROPS,
    },
    WidgetSchema {
        kind_name: "Button",
        description: "An application-semantic interactive button delivering a ButtonPressed{source} event (ADR-015).",
        safety_critical_eligible: true,
        properties: BUTTON_PROPS,
    },
    WidgetSchema {
        kind_name: "TextInput",
        description: "An operator-editable, controlled-component text field over a baked approved charset (ADR-015).",
        safety_critical_eligible: true,
        properties: TEXT_INPUT_PROPS,
    },
];

/// The governed widget catalog: one entry per `.medui` component kind, in the same order
/// `docs/dsl/component-dictionary.md` documents them. Source of truth for required/optional
/// properties: that document and the `parse_component_properties` match in `src/lib.rs`.
pub fn widget_catalog() -> &'static [WidgetSchema] {
    WIDGET_CATALOG
}

/// One approved text key's value and measured pixel bounds in every locale it is approved for —
/// lets a GUI show a per-locale text-budget overrun before ever invoking the compiler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocaleEntry {
    pub locale: String,
    pub value: String,
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextKeyInfo {
    pub string_id: String,
    pub entries: Vec<LocaleEntry>,
}

/// Enumerates every approved string id in `package`, each with its resolved value and measured
/// ink bounds in every locale the package declares a compiled run for.
pub fn enumerate_text_keys(package: &TextPackage) -> Vec<TextKeyInfo> {
    let mut string_ids: Vec<&str> = package
        .approved_strings
        .iter()
        .map(|approved| approved.id.as_str())
        .collect();
    string_ids.sort_unstable();
    string_ids.dedup();

    string_ids
        .into_iter()
        .map(|string_id| {
            let mut entries = Vec::new();
            for locale in package.locales() {
                let Some(approved) = package.find_approved_string(string_id, &locale) else {
                    continue;
                };
                let Some(run) = package.find_run_for_string(string_id, &locale) else {
                    continue;
                };
                let Ok(bounds) = package.measure_run_bounds(run) else {
                    continue;
                };
                entries.push(LocaleEntry {
                    locale,
                    value: approved.value.clone(),
                    width_px: bounds.width(),
                    height_px: bounds.height(),
                });
            }
            TextKeyInfo {
                string_id: string_id.to_string(),
                entries,
            }
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NumericTemplateInfo {
    pub id: String,
    pub locale: String,
    pub max_chars: u8,
    pub glyph_set_id: String,
}

/// Enumerates every `NumericTemplate` a `NumericDisplay` widget's `template:` property may
/// reference, so a GUI can offer a closed dropdown instead of a free-typed template id.
pub fn enumerate_numeric_templates(package: &TextPackage) -> Vec<NumericTemplateInfo> {
    package
        .numeric_templates
        .iter()
        .map(|template| NumericTemplateInfo {
            id: template.id.clone(),
            locale: template.locale.clone(),
            max_chars: template.max_chars,
            glyph_set_id: template.glyph_set_id.clone(),
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageInfo {
    pub id: String,
    pub width: u32,
    pub height: u32,
}

/// Enumerates every baked image an `Image` widget's `source: img("...")` property may reference.
/// Intrinsic size is load-bearing: the compiler requires a declared `Image` node's `width`/
/// `height` to equal it exactly (`crates/trustsc-ui/src/lib.rs`), so the palette must pre-size
/// image drops rather than let a GUI guess dimensions.
pub fn enumerate_images(images: &[ImagePackage]) -> Vec<ImageInfo> {
    images
        .iter()
        .map(|package| ImageInfo {
            id: package.id.clone(),
            width: package.width,
            height: package.height,
        })
        .collect()
}
