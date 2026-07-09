#![forbid(unsafe_code)]

//! `mdux-ui-verify`: the pure, dependency-free check engine that gives golden references their
//! first consumer (ADR-016 §2). It takes a captured frame's raw pixels plus the compiled screen
//! (and per-locale expectations the caller resolves) and returns typed [`CheckResult`]s — no GPU,
//! no window, no external crate. That makes it fully unit-testable on synthetic pixel buffers
//! and reusable by the external safety monitors `docs/dsl/safety-monitor-contract.md`
//! anticipates.
//!
//! The check vocabulary ([`CheckKind`]) is exactly the one ADR-016 §2/§3 defines: `GoldenBounds`,
//! `ChromeColor`, `TextPresence`, `InkContainment` and `ColorHash`. [`verify_frame`] runs every
//! applicable check against a captured frame; [`emit_report_json`] renders a
//! [`VerificationReport`] as byte-reproducible JSON with no serde in governed code.

mod checks;
mod report;
mod sha256;

use std::collections::HashMap;

use mdux_ui::{CompiledNodeKind, CompiledScreenPackage, CvCheckKind, Rect};

pub use report::{ScenarioTraceRow, TraceRow, VerificationReport, emit_report_json};

/// One captured frame's tightly packed, row-major RGBA8 pixels — the same layout ADR-016 §1's
/// offscreen `read_pixels()` produces.
#[derive(Clone, Copy, Debug)]
pub struct FramePixels<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

impl<'a> FramePixels<'a> {
    /// The pixel at `(x, y)`, or `None` if the coordinate falls outside the frame or the pixel
    /// buffer is shorter than `width * height * 4` bytes at that offset.
    pub fn pixel(&self, x: i32, y: i32) -> Option<[u8; 4]> {
        if x < 0 || y < 0 {
            return None;
        }
        let (x, y) = (x as u32, y as u32);
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = ((y * self.width + x) as usize) * 4;
        let slice = self.rgba.get(index..index + 4)?;
        Some([slice[0], slice[1], slice[2], slice[3]])
    }
}

/// The check vocabulary this engine implements (ADR-016 §2).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CheckKind {
    GoldenBounds,
    ChromeColor,
    TextPresence,
    InkContainment,
    ColorHash,
}

impl CheckKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckKind::GoldenBounds => "golden_bounds",
            CheckKind::ChromeColor => "chrome_color",
            CheckKind::TextPresence => "text_presence",
            CheckKind::InkContainment => "ink_containment",
            CheckKind::ColorHash => "color_hash",
        }
    }
}

/// `NoBaseline` is informational only — it must never be treated as a pass (ADR-016 §3).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CheckOutcome {
    Pass,
    Fail,
    NoBaseline,
}

impl CheckOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckOutcome::Pass => "pass",
            CheckOutcome::Fail => "fail",
            CheckOutcome::NoBaseline => "no_baseline",
        }
    }

    /// `true` only for [`Pass`](Self::Pass) — the single predicate a CI gate should use, since
    /// treating [`NoBaseline`](Self::NoBaseline) as anything but "not a pass" would silently
    /// defeat the Tier-2 honesty guarantee ADR-016 §3 describes.
    pub fn is_pass(&self) -> bool {
        matches!(self, CheckOutcome::Pass)
    }
}

/// Expected and measured values for one check, kept concrete and integer-only (parts-per-million
/// for coverage, not floats) so reports stay deterministic across platforms.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckPayload {
    ChromeColor {
        /// The color token the expected value derives from — for `TextInput` this documents
        /// the actual chrome derivation (`Theme.Colors.Neutral*0.35`), not `TextInputSpec`'s own
        /// `color_token` (which tints the caret instead).
        expected_token: String,
        expected_rgba: [u8; 4],
        measured_rgba: [u8; 4],
        max_channel_delta: u8,
        sample_count: u32,
    },
    GoldenBounds {
        expected_bounds: Rect,
        /// `None` when no ink was found in the search region at all (vacuously contained).
        measured_ink_bounds: Option<Rect>,
        contained: bool,
    },
    TextPresence {
        coverage_ppm: u32,
        expected_min_ppm: u32,
        expected_max_ppm: u32,
    },
    InkContainment {
        outside_ink_pixels: u32,
    },
    ColorHash {
        /// `None` exactly when the outcome is [`CheckOutcome::NoBaseline`].
        expected_hex: Option<String>,
        measured_hex: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckResult {
    pub check_id: String,
    pub node_id: String,
    pub kind: CheckKind,
    pub requirement_id: Option<String>,
    pub outcome: CheckOutcome,
    pub payload: CheckPayload,
}

/// Per-locale expectations the caller resolves before calling [`verify_frame`]: the frame's
/// clear/background color, the active locale's compiled glyph count for every text-bearing node
/// (static or scenario-known dynamic — this engine does not distinguish the two, since it only
/// depends on `mdux-ui`, not the text runtime), and any committed `ColorHash` baseline per node.
/// A node absent from the glyph-count map is skipped for `TextPresence`; a golden reference
/// absent from the baseline map reports `NoBaseline` for `ColorHash`, never a pass.
#[derive(Clone, Debug, Default)]
pub struct FrameExpectations {
    background_rgba: [u8; 4],
    glyph_counts: HashMap<String, u32>,
    color_hash_baselines: HashMap<String, String>,
}

impl FrameExpectations {
    pub fn new(background_rgba: [u8; 4]) -> Self {
        Self {
            background_rgba,
            glyph_counts: HashMap::new(),
            color_hash_baselines: HashMap::new(),
        }
    }

    pub fn with_glyph_count(mut self, node_id: impl Into<String>, glyph_count: u32) -> Self {
        self.glyph_counts.insert(node_id.into(), glyph_count);
        self
    }

    pub fn with_color_hash_baseline(mut self, node_id: impl Into<String>, baseline_hex: impl Into<String>) -> Self {
        self.color_hash_baselines.insert(node_id.into(), baseline_hex.into());
        self
    }

    pub fn background_rgba(&self) -> [u8; 4] {
        self.background_rgba
    }

    pub fn glyph_count(&self, node_id: &str) -> Option<u32> {
        self.glyph_counts.get(node_id).copied()
    }

    pub fn color_hash_baseline(&self, node_id: &str) -> Option<&str> {
        self.color_hash_baselines.get(node_id).map(String::as_str)
    }
}

/// A generous, documented heuristic for how many ink pixels (parts-per-million of `bounds`'
/// area) a rendered run of `glyph_count` glyphs should occupy — wide enough to span condensed and
/// bold fonts, punctuation-heavy and dense text, without false-failing on any real rendered
/// glyph set, while still catching a blank region (too little ink) or garbled/overflowing text
/// (way too much ink).
///
/// Model: each glyph is assumed to occupy a roughly square cell whose side is half of `bounds`'
/// height (a typical glyph aspect ratio), and to paint somewhere between 5% (a sparse glyph like
/// `.` or a narrow `l`, averaged in with spaces) and 70% (a dense, bold glyph like `M` or `@`) of
/// that cell with ink. `(0, 0)` when there is no area or no glyphs to check.
pub fn coverage_band_for_glyphs(glyph_count: u32, bounds: Rect) -> (u32, u32) {
    let area = u64::from(bounds.width) * u64::from(bounds.height);
    if area == 0 || glyph_count == 0 {
        return (0, 0);
    }

    let cell_side = (u64::from(bounds.height) / 2).max(1);
    let glyph_cell_area = cell_side * cell_side;
    let min_ink = glyph_cell_area * u64::from(glyph_count) * 5 / 100;
    let max_ink = glyph_cell_area * u64::from(glyph_count) * 70 / 100;

    let min_ppm = ((min_ink.saturating_mul(1_000_000)) / area).min(1_000_000) as u32;
    let max_ppm = ((max_ink.saturating_mul(1_000_000)) / area).min(1_000_000) as u32;
    (min_ppm, max_ppm.max(min_ppm))
}

/// `sha256(RGBA8 bytes of the rect `bounds`, row-major, top-to-bottom, tightly packed)` per
/// ADR-016 §3, rendered as 64 lowercase hex characters. Pixels of `bounds` that fall outside
/// `frame` (which should not happen for a golden reference compiled against this frame's own
/// surface extent) contribute four zero bytes each, keeping the function total and deterministic
/// rather than panicking.
pub fn color_hash(frame: FramePixels, bounds: Rect) -> String {
    let mut buffer = Vec::with_capacity((bounds.width as usize) * (bounds.height as usize) * 4);
    for y in bounds.y..(bounds.y + bounds.height as i32) {
        for x in bounds.x..(bounds.x + bounds.width as i32) {
            let pixel = frame.pixel(x, y).unwrap_or([0, 0, 0, 0]);
            buffer.extend_from_slice(&pixel);
        }
    }
    sha256::sha256_hex(&buffer)
}

/// Runs every applicable check from ADR-016 §2 against `frame`: `ChromeColor` and
/// `InkContainment` for every node (the latter skipped for `Panel`, which is an underlay by
/// definition and exempt from the ADR-014 overlap rule), `TextPresence` for every node
/// `expectations` declares a glyph count for, and `GoldenBounds`/`ColorHash` for every golden
/// reference entry that declares the corresponding `CvCheckKind`.
pub fn verify_frame(
    screen: &CompiledScreenPackage,
    frame: FramePixels,
    expectations: &FrameExpectations,
) -> Vec<CheckResult> {
    let mut results = Vec::new();

    for node in screen.nodes {
        let requirement_id = node.kind.requirement_id().map(str::to_string);

        if let Some(result) = checks::chrome_color_check(node, &frame, requirement_id.clone()) {
            results.push(result);
        }

        if !matches!(node.kind, CompiledNodeKind::Panel(_)) {
            results.push(checks::ink_containment_check(
                node,
                screen.nodes,
                &frame,
                expectations,
                requirement_id.clone(),
            ));
        }

        if let Some(glyph_count) = expectations.glyph_count(node.id) {
            results.push(checks::text_presence_check(
                node,
                glyph_count,
                &frame,
                expectations,
                requirement_id.clone(),
            ));
        }
    }

    for entry in screen.golden_references {
        let requirement_id = screen
            .find_node(entry.node_id)
            .and_then(|node| node.kind.requirement_id())
            .map(str::to_string);

        if checks::declares_cv_check(entry, CvCheckKind::Bounds) {
            results.push(checks::golden_bounds_check(
                entry,
                &frame,
                expectations,
                requirement_id.clone(),
            ));
        }
        if checks::declares_cv_check(entry, CvCheckKind::ColorHash) {
            results.push(checks::color_hash_check(entry, frame, expectations, requirement_id.clone()));
        }
    }

    results
}

#[cfg(test)]
mod tests;
