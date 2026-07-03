//! Bounded realtime screen bindings (ADR-013): the dynamic counterpart of
//! [`crate::screen_text::ScreenTextLayout`]. Where the static layout resolves approved runs once
//! at startup, this module resolves *where* each realtime element renders (clock, numeric
//! display, status indicator, streaming viewport), from *which* template/glyph set, with *what*
//! fixed capacity — so the presentation adapter stays a dumb executor and applications stay at
//! `frame.set_number("SEDATION_INDEX", 42)`.
//!
//! Everything is sized at construction: the per-frame API ([`FrameInputs`]) allocates nothing.

use mdux_core::{MduxResult, ValidationError};
use mdux_text_schema::TextPackage;
use mdux_ui::{ClockFormat, CompiledNodeKind, CompiledScreenPackage, Rect};

/// Default ring-buffer dimensions for a streaming viewport (rows of history × bins per row).
pub const DEFAULT_STREAM_ROWS: usize = 64;
pub const DEFAULT_STREAM_BINS: usize = 64;

/// Where and how a `Clock` node renders. The adapter feeds it from the platform clock; the
/// application writes no code for it.
#[derive(Clone, Debug, PartialEq)]
pub struct ClockBinding {
    pub node_id: &'static str,
    pub bounds: Rect,
    pub origin_x: i32,
    pub origin_y: i32,
    pub format: ClockFormat,
    pub glyph_set_id: String,
    /// Maximum glyph draw commands one render of this clock can produce.
    pub capacity: usize,
}

/// Where and how a `NumericDisplay` node renders, bound to its realtime `source`.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberBinding {
    pub node_id: &'static str,
    pub bounds: Rect,
    pub origin_x: i32,
    pub origin_y: i32,
    pub source: &'static str,
    pub template_id: String,
    pub color_token: &'static str,
    /// Maximum glyph draw commands (max_chars digits + affix run glyphs).
    pub capacity: usize,
}

/// Where and how a `StatusIndicator` node renders: one resolved approved run per state, selected
/// by index at runtime.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusBinding {
    pub node_id: &'static str,
    pub bounds: Rect,
    pub source: &'static str,
    /// Per-state resolved run ids (standard package, binding locale), index-aligned with the
    /// screen's `state_text_keys` and `color_tokens`.
    pub state_run_ids: Vec<String>,
    /// Per-state render origins (`bounds - run_bounds.min`, like the static layout).
    pub state_origins: Vec<(i32, i32)>,
    pub color_tokens: &'static [&'static str],
    /// Maximum glyph draw commands across all states.
    pub capacity: usize,
}

/// A streaming viewport's declared ring buffer: `rows` history rows of `bins` samples, rendered
/// as the 3D DSA waterfall inside `bounds`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamBinding {
    pub node_id: &'static str,
    pub source: &'static str,
    pub bounds: Rect,
    pub rows: usize,
    pub bins: usize,
}

/// Every realtime binding resolved from a compiled screen, plus the packages they render from.
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenBindings {
    pub standard: TextPackage,
    pub display: TextPackage,
    pub locale: String,
    pub clocks: Vec<ClockBinding>,
    pub numbers: Vec<NumberBinding>,
    pub statuses: Vec<StatusBinding>,
    pub streams: Vec<StreamBinding>,
}

impl ScreenBindings {
    /// Resolves every dynamic node of `screen`: `Clock` against the standard package's clock
    /// glyph set, `NumericDisplay` against its template in the display package,
    /// `StatusIndicator` against its per-state approved runs for `locale`, and every
    /// `VulkanViewport` as a stream declaration with the default ring dimensions.
    pub fn from_screen(
        screen: &'static CompiledScreenPackage,
        standard: TextPackage,
        display: TextPackage,
        locale: &str,
    ) -> MduxResult<Self> {
        let mut clocks = Vec::new();
        let mut numbers = Vec::new();
        let mut statuses = Vec::new();
        let mut streams = Vec::new();

        for node in screen.nodes {
            match &node.kind {
                CompiledNodeKind::Clock(spec) => {
                    let glyph_set_id = crate::DEFAULT_STANDARD_DIGITS_GLYPH_SET_ID.to_string();
                    let glyph_set = standard
                        .find_numeric_glyph_set(&glyph_set_id)
                        .ok_or_else(|| {
                            ValidationError::new(format!(
                                "clock {} requires glyph set {glyph_set_id} in the standard package",
                                node.id
                            ))
                        })?;
                    for required in ['0', ':', '-', ' '] {
                        if !glyph_set
                            .entries
                            .iter()
                            .any(|entry| entry.character == required)
                        {
                            return Err(ValidationError::new(format!(
                                "clock {} glyph set {glyph_set_id} is missing '{required}'",
                                node.id
                            )));
                        }
                    }
                    let glyph_height = max_glyph_height(&standard, &glyph_set_id)?;
                    let capacity = match spec.format {
                        ClockFormat::TimeSeconds => 8,
                        ClockFormat::DateTimeSeconds => 19,
                    };
                    clocks.push(ClockBinding {
                        node_id: node.id,
                        bounds: node.bounds,
                        origin_x: node.bounds.x,
                        origin_y: centered_origin_y(node.bounds, glyph_height),
                        format: spec.format,
                        glyph_set_id,
                        capacity,
                    });
                }
                CompiledNodeKind::NumericDisplay(spec) => {
                    let template = display.find_template(spec.template_id).ok_or_else(|| {
                        ValidationError::new(format!(
                            "numeric display {} references unknown template {} in the display package",
                            node.id, spec.template_id
                        ))
                    })?;
                    let mut capacity = usize::from(template.max_chars);
                    for affix_run_id in
                        [&template.prefix_run_id, &template.suffix_run_id].into_iter().flatten()
                    {
                        let run = display.find_run(affix_run_id).ok_or_else(|| {
                            ValidationError::new(format!(
                                "numeric display {} template references unknown run {affix_run_id}",
                                node.id
                            ))
                        })?;
                        capacity += run.glyphs.len();
                    }
                    let glyph_height = max_glyph_height(&display, &template.glyph_set_id)?;
                    numbers.push(NumberBinding {
                        node_id: node.id,
                        bounds: node.bounds,
                        origin_x: node.bounds.x,
                        origin_y: centered_origin_y(node.bounds, glyph_height),
                        source: spec.source,
                        template_id: spec.template_id.to_string(),
                        color_token: spec.color_token,
                        capacity,
                    });
                }
                CompiledNodeKind::StatusIndicator(spec) => {
                    let mut state_run_ids = Vec::with_capacity(spec.state_text_keys.len());
                    let mut state_origins = Vec::with_capacity(spec.state_text_keys.len());
                    let mut capacity = 0usize;
                    for state_text_key in spec.state_text_keys {
                        let run = standard
                            .find_run_for_string(state_text_key, locale)
                            .ok_or_else(|| {
                                ValidationError::new(format!(
                                    "status indicator {} state {state_text_key} has no compiled run for locale {locale}",
                                    node.id
                                ))
                            })?;
                        let run_bounds = standard.measure_run_bounds(run)?;
                        let origin_x =
                            node.bounds.x.checked_sub(run_bounds.min_x).ok_or_else(|| {
                                ValidationError::new(format!(
                                    "status indicator {} origin x is out of i32 range",
                                    node.id
                                ))
                            })?;
                        let origin_y =
                            node.bounds.y.checked_sub(run_bounds.min_y).ok_or_else(|| {
                                ValidationError::new(format!(
                                    "status indicator {} origin y is out of i32 range",
                                    node.id
                                ))
                            })?;
                        capacity = capacity.max(run.glyphs.len());
                        state_run_ids.push(run.id.clone());
                        state_origins.push((origin_x, origin_y));
                    }
                    statuses.push(StatusBinding {
                        node_id: node.id,
                        bounds: node.bounds,
                        source: spec.source,
                        state_run_ids,
                        state_origins,
                        color_tokens: spec.color_tokens,
                        capacity,
                    });
                }
                CompiledNodeKind::VulkanViewport(spec) => {
                    streams.push(StreamBinding {
                        node_id: node.id,
                        source: spec.stream_source,
                        bounds: node.bounds,
                        rows: DEFAULT_STREAM_ROWS,
                        bins: DEFAULT_STREAM_BINS,
                    });
                }
                CompiledNodeKind::CriticalButton(_) | CompiledNodeKind::Label(_) => {}
            }
        }

        Ok(Self {
            standard,
            display,
            locale: locale.to_string(),
            clocks,
            numbers,
            statuses,
            streams,
        })
    }

    /// The renderer sizes its dynamic vertex buffer from this: the worst-case total glyph draw
    /// commands one frame can produce across every realtime text binding.
    pub fn max_dynamic_quads(&self) -> usize {
        self.clocks.iter().map(|binding| binding.capacity).sum::<usize>()
            + self.numbers.iter().map(|binding| binding.capacity).sum::<usize>()
            + self.statuses.iter().map(|binding| binding.capacity).sum::<usize>()
    }
}

fn centered_origin_y(bounds: Rect, glyph_height: u32) -> i32 {
    bounds.y + (bounds.height.saturating_sub(glyph_height) / 2) as i32
}

fn max_glyph_height(package: &TextPackage, glyph_set_id: &str) -> MduxResult<u32> {
    let glyph_set = package.find_numeric_glyph_set(glyph_set_id).ok_or_else(|| {
        ValidationError::new(format!("unknown numeric glyph set {glyph_set_id}"))
    })?;
    let mut height = 0u32;
    for entry in &glyph_set.entries {
        if let Some(atlas_glyph) = package.find_glyph(entry.atlas_index, entry.glyph_id) {
            height = height.max(u32::from(atlas_glyph.height));
        }
    }
    Ok(height)
}

/// The bounded per-frame mailbox: the application's realtime closure writes into it, the
/// adapter drains it before recording each frame. All storage is allocated at construction from
/// the screen's bindings; the setters allocate nothing and reject unknown sources with typed
/// errors. Values persist between frames until overwritten.
#[derive(Clone, Debug)]
pub struct FrameInputs {
    numbers: Vec<(&'static str, i64)>,
    statuses: Vec<(&'static str, u8, usize)>, // (source, state_index, state_count)
    streams: Vec<StreamState>,
}

#[derive(Clone, Debug)]
struct StreamState {
    source: &'static str,
    rows: usize,
    bins: usize,
    /// Ring storage, `rows × bins`, normalized samples.
    data: Vec<f32>,
    /// Physical row the *next* push writes to.
    cursor: usize,
}

impl FrameInputs {
    pub fn from_bindings(bindings: &ScreenBindings) -> Self {
        Self {
            numbers: bindings
                .numbers
                .iter()
                .map(|binding| (binding.source, 0i64))
                .collect(),
            statuses: bindings
                .statuses
                .iter()
                .map(|binding| (binding.source, 0u8, binding.state_run_ids.len()))
                .collect(),
            streams: bindings
                .streams
                .iter()
                .map(|binding| StreamState {
                    source: binding.source,
                    rows: binding.rows,
                    bins: binding.bins,
                    data: vec![0.0; binding.rows * binding.bins],
                    cursor: 0,
                })
                .collect(),
        }
    }

    /// Sets the current value of a `NumericDisplay` source. The value is range-checked at render
    /// time against the template (`max_chars`, `allow_negative`).
    pub fn set_number(&mut self, source: &str, value: i64) -> MduxResult<()> {
        let slot = self
            .numbers
            .iter_mut()
            .find(|(slot_source, _)| *slot_source == source)
            .ok_or_else(|| {
                ValidationError::new(format!("unknown numeric source {source}"))
            })?;
        slot.1 = value;
        Ok(())
    }

    /// Selects the active state of a `StatusIndicator` source by index.
    pub fn set_status(&mut self, source: &str, state_index: u8) -> MduxResult<()> {
        let slot = self
            .statuses
            .iter_mut()
            .find(|(slot_source, _, _)| *slot_source == source)
            .ok_or_else(|| {
                ValidationError::new(format!("unknown status source {source}"))
            })?;
        if usize::from(state_index) >= slot.2 {
            return Err(ValidationError::new(format!(
                "status source {source} has {} states, index {state_index} is out of range",
                slot.2
            )));
        }
        slot.1 = state_index;
        Ok(())
    }

    /// Pushes one spectrum row into a stream's ring buffer, overwriting the oldest row once the
    /// ring is full. `row.len()` must equal the stream's declared `bins`.
    pub fn push_row(&mut self, source: &str, row: &[f32]) -> MduxResult<()> {
        let stream = self
            .streams
            .iter_mut()
            .find(|stream| stream.source == source)
            .ok_or_else(|| {
                ValidationError::new(format!("unknown stream source {source}"))
            })?;
        if row.len() != stream.bins {
            return Err(ValidationError::new(format!(
                "stream {source} expects rows of {} bins, got {}",
                stream.bins,
                row.len()
            )));
        }
        let start = stream.cursor * stream.bins;
        stream.data[start..start + stream.bins].copy_from_slice(row);
        stream.cursor = (stream.cursor + 1) % stream.rows;
        Ok(())
    }

    /// The current value of a numeric source (renderer-side read).
    pub fn number(&self, source: &str) -> Option<i64> {
        self.numbers
            .iter()
            .find(|(slot_source, _)| *slot_source == source)
            .map(|(_, value)| *value)
    }

    /// The active state index of a status source (renderer-side read).
    pub fn status_index(&self, source: &str) -> Option<u8> {
        self.statuses
            .iter()
            .find(|(slot_source, _, _)| *slot_source == source)
            .map(|(_, index, _)| *index)
    }

    /// The ring storage and write cursor of a stream (renderer-side read): `data` is
    /// `rows × bins` row-major, `cursor` is the physical row the next push will overwrite —
    /// i.e. the *oldest* row currently on screen.
    pub fn stream(&self, source: &str) -> Option<(&[f32], usize)> {
        self.streams
            .iter()
            .find(|stream| stream.source == source)
            .map(|stream| (stream.data.as_slice(), stream.cursor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CompiledNode, CompiledScreenPackage, LayoutKind, LayoutSpec, NumericDisplaySpec,
        StatusIndicatorSpec, ViewportReservation, default_display_text_package,
        default_standard_text_package,
    };
    use mdux_ui::ClockSpec;

    const MONITOR_SCREEN: CompiledScreenPackage = CompiledScreenPackage {
        screen_id: "BindingsTest",
        layout: LayoutSpec {
            kind: LayoutKind::Vertical,
            spacing: 8,
            padding: 16,
        },
        nodes: &[
            CompiledNode {
                id: "wall-clock",
                bounds: Rect { x: 372, y: 16, width: 676, height: 48 },
                kind: CompiledNodeKind::Clock(ClockSpec {
                    format: ClockFormat::DateTimeSeconds,
                }),
            },
            CompiledNode {
                id: "sedation-index",
                bounds: Rect { x: 16, y: 72, width: 1248, height: 120 },
                kind: CompiledNodeKind::NumericDisplay(NumericDisplaySpec {
                    requirement_id: "REQ-NS-001",
                    template_id: "TPL-SEDATION-INDEX",
                    source: "SEDATION_INDEX",
                    color_token: "Theme.Colors.ScoreDigits",
                }),
            },
            CompiledNode {
                id: "system-status",
                bounds: Rect { x: 1064, y: 16, width: 200, height: 48 },
                kind: CompiledNodeKind::StatusIndicator(StatusIndicatorSpec {
                    requirement_id: "REQ-NS-003",
                    source: "MONITOR_STATUS",
                    state_text_keys: &["STR-NS-NOMINAL", "STR-NS-ALERT", "STR-NS-FAULT"],
                    color_tokens: &["Theme.Colors.A", "Theme.Colors.B", "Theme.Colors.C"],
                }),
            },
            CompiledNode {
                id: "eeg-dsa",
                bounds: Rect { x: 16, y: 200, width: 1248, height: 504 },
                kind: CompiledNodeKind::VulkanViewport(ViewportReservation {
                    stream_source: "EEG_DSA",
                }),
            },
        ],
        golden_references: &[],
    };

    fn bindings() -> ScreenBindings {
        ScreenBindings::from_screen(
            &MONITOR_SCREEN,
            default_standard_text_package().expect("standard package"),
            default_display_text_package().expect("display package"),
            "en-US",
        )
        .expect("bindings should resolve")
    }

    #[test]
    fn resolves_one_binding_per_dynamic_node_with_documented_capacities() {
        let bindings = bindings();

        assert_eq!(bindings.clocks.len(), 1);
        assert_eq!(bindings.clocks[0].capacity, 19); // YYYY-MM-DD HH:MM:SS
        assert_eq!(bindings.numbers.len(), 1);
        assert_eq!(bindings.numbers[0].capacity, 2); // affixless, max_chars = 2
        assert_eq!(bindings.statuses.len(), 1);
        assert_eq!(bindings.statuses[0].state_run_ids.len(), 3);
        assert_eq!(bindings.streams.len(), 1);
        assert_eq!(bindings.streams[0].rows, DEFAULT_STREAM_ROWS);
        assert_eq!(bindings.streams[0].bins, DEFAULT_STREAM_BINS);

        // NOMINAL = 7 glyphs is the widest en-US state label.
        assert_eq!(bindings.statuses[0].capacity, 7);
        assert_eq!(bindings.max_dynamic_quads(), 19 + 2 + 7);
    }

    #[test]
    fn frame_inputs_validate_sources_and_ranges() {
        let bindings = bindings();
        let mut inputs = FrameInputs::from_bindings(&bindings);

        inputs.set_number("SEDATION_INDEX", 47).expect("known source");
        assert_eq!(inputs.number("SEDATION_INDEX"), Some(47));

        let error = inputs
            .set_number("UNKNOWN", 1)
            .expect_err("unknown source rejected");
        assert!(error.to_string().contains("unknown numeric source"));

        inputs.set_status("MONITOR_STATUS", 2).expect("state 2 exists");
        let error = inputs
            .set_status("MONITOR_STATUS", 3)
            .expect_err("state 3 out of range");
        assert!(error.to_string().contains("out of range"));

        let row = vec![0.5f32; DEFAULT_STREAM_BINS];
        inputs.push_row("EEG_DSA", &row).expect("row of declared width");
        let error = inputs
            .push_row("EEG_DSA", &[0.0; 3])
            .expect_err("wrong row width rejected");
        assert!(error.to_string().contains("expects rows of"));

        let (data, cursor) = inputs.stream("EEG_DSA").expect("stream exists");
        assert_eq!(cursor, 1);
        assert_eq!(data[0], 0.5);
    }

    #[test]
    fn stream_ring_wraps_and_overwrites_oldest() {
        let bindings = bindings();
        let mut inputs = FrameInputs::from_bindings(&bindings);

        for index in 0..(DEFAULT_STREAM_ROWS + 2) {
            let row = vec![index as f32; DEFAULT_STREAM_BINS];
            inputs.push_row("EEG_DSA", &row).expect("push");
        }

        let (data, cursor) = inputs.stream("EEG_DSA").expect("stream exists");
        assert_eq!(cursor, 2); // wrapped past the end twice
        assert_eq!(data[0], DEFAULT_STREAM_ROWS as f32); // physical row 0 overwritten
        assert_eq!(data[DEFAULT_STREAM_BINS], (DEFAULT_STREAM_ROWS + 1) as f32);
    }
}
