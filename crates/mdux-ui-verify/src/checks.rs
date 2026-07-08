//! Check implementations (ADR-016 §2). Every function here is pure: it reads pixels out of a
//! [`FramePixels`] and compiled/expected data, and returns a [`CheckResult`] with concrete
//! expected/measured payloads. No function in this module allocates GPU state or touches
//! anything outside its arguments.

use mdux_ui::{CompiledNode, CompiledNodeKind, CvCheckKind, GoldenReferenceEntry, Rect, resolve_color_token};

use crate::{CheckKind, CheckOutcome, CheckPayload, CheckResult, FrameExpectations, FramePixels, color_hash};

/// Per-channel background-differencing threshold used by [`GoldenBounds`](CheckKind::GoldenBounds)
/// and [`InkContainment`](CheckKind::InkContainment) to decide whether a pixel is "ink" (part of
/// the node's own rendered content) rather than background: any channel differing from the
/// expected background by more than this many UNORM steps counts as ink. Chosen generously above
/// antialiasing noise (which stays within a few steps of the background near a glyph edge) while
/// staying far below any real content color, which differs from the surrounding background by
/// tens to hundreds of steps in this theme table.
const INK_DELTA: u8 = 8;

/// Search-region margin (pixels) added around a golden reference's declared bounds before
/// looking for ink: wide enough to catch a real rendering overflow, tight enough to stay cheap.
const GOLDEN_SEARCH_MARGIN: i32 = 8;

/// Margin (pixels) added around a node's bounds when checking for ink that escaped its box —
/// the same figure as [`GOLDEN_SEARCH_MARGIN`] for the same reason (ADR-014 bounds are already
/// statically disjoint, so this just needs to be wider than any plausible overflow).
const CONTAINMENT_MARGIN: i32 = 8;

/// Per-channel UNORM tolerance for [`ChromeColor`](CheckKind::ChromeColor): rounding a theme
/// float to a byte can land the expected value off by one step from what an implementation
/// legitimately rounds to, per ADR-016 §2.
const CHROME_TOLERANCE: u8 = 1;

/// Panel interior inset (pixels): panels are solid underlays with no label, so 1px away from the
/// declared edge is already clear of any border-antialiasing artifact.
const PANEL_INSET: i32 = 1;

/// Button/CriticalButton face inset (pixels) from every edge before sampling — keeps the
/// sampled band off the outer edge's antialiasing.
const BUTTON_EDGE_INSET: i32 = 4;

/// Height (pixels) of each sampled edge band on a button face. Two bands are sampled — one just
/// inside the top edge, one just inside the bottom edge — because the label is centered
/// vertically (ADR-015 layout) and never reaches these bands for any realistic button size.
const BUTTON_BAND_HEIGHT: i32 = 6;

/// Text-input field inset (pixels): identical reasoning to [`BUTTON_EDGE_INSET`].
const TEXT_INPUT_EDGE_INSET: i32 = 4;

/// Text-input sampled band height (pixels): identical reasoning to [`BUTTON_BAND_HEIGHT`]. Field
/// content is left-aligned text plus a caret, both vertically centered, so top/bottom bands stay
/// clear of them too.
const TEXT_INPUT_BAND_HEIGHT: i32 = 6;

fn is_ink(pixel: [u8; 4], background: [u8; 4]) -> bool {
    pixel
        .iter()
        .zip(background.iter())
        .any(|(&sample, &back)| sample.abs_diff(back) > INK_DELTA)
}

fn channelwise_max_delta(measured: [u8; 4], expected: [u8; 4]) -> u8 {
    measured
        .iter()
        .zip(expected.iter())
        .map(|(&sample, &target)| sample.abs_diff(target))
        .max()
        .unwrap_or(0)
}

fn point_in_rect(x: i32, y: i32, rect: Rect) -> bool {
    x >= rect.x
        && y >= rect.y
        && x < rect.x + rect.width as i32
        && y < rect.y + rect.height as i32
}

fn rect_contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width as i32 <= outer.x + outer.width as i32
        && inner.y + inner.height as i32 <= outer.y + outer.height as i32
}

/// Expands `rect` by `margin` on every side and clamps the result to `0..frame_width` /
/// `0..frame_height`, so a search region always stays inside the frame that was actually
/// captured.
fn expand_and_clamp(rect: Rect, margin: i32, frame_width: u32, frame_height: u32) -> Rect {
    let x0 = (rect.x - margin).max(0);
    let y0 = (rect.y - margin).max(0);
    let x1 = (rect.x + rect.width as i32 + margin)
        .min(frame_width as i32)
        .max(x0);
    let y1 = (rect.y + rect.height as i32 + margin)
        .min(frame_height as i32)
        .max(y0);
    Rect {
        x: x0,
        y: y0,
        width: (x1 - x0) as u32,
        height: (y1 - y0) as u32,
    }
}

/// Shrinks `rect` by `amount` on every side; `None` if nothing sensible remains.
fn inset(rect: Rect, amount: i32) -> Option<Rect> {
    let width = rect.width as i32 - 2 * amount;
    let height = rect.height as i32 - 2 * amount;
    if width <= 0 || height <= 0 {
        return None;
    }
    Some(Rect {
        x: rect.x + amount,
        y: rect.y + amount,
        width: width as u32,
        height: height as u32,
    })
}

/// Two horizontal bands just inside the top and bottom edges of `bounds`, both clear of a
/// vertically centered label/caret. `None` if `bounds` is too small to fit the insets and bands
/// without the two bands overlapping.
fn edge_bands(bounds: Rect, edge_inset: i32, band_height: i32) -> Option<[Rect; 2]> {
    let x = bounds.x + edge_inset;
    let width = bounds.width as i32 - 2 * edge_inset;
    if width <= 0 {
        return None;
    }
    let min_height_needed = 2 * edge_inset + 2 * band_height;
    if (bounds.height as i32) < min_height_needed {
        return None;
    }
    let top = Rect {
        x,
        y: bounds.y + edge_inset,
        width: width as u32,
        height: band_height as u32,
    };
    let bottom = Rect {
        x,
        y: bounds.y + bounds.height as i32 - edge_inset - band_height,
        width: width as u32,
        height: band_height as u32,
    };
    Some([top, bottom])
}

fn theme_byte(component: f32) -> u8 {
    let scaled = component * 255.0;
    let rounded = (scaled + 0.5).floor();
    if rounded <= 0.0 {
        0
    } else if rounded >= 255.0 {
        255
    } else {
        rounded as u8
    }
}

fn theme_bytes(rgba: [f32; 4]) -> [u8; 4] {
    [
        theme_byte(rgba[0]),
        theme_byte(rgba[1]),
        theme_byte(rgba[2]),
        theme_byte(rgba[3]),
    ]
}

/// Resolves the glyph-free chrome sampling regions for one node, per ADR-016 §2: derived from
/// the compiled node kind, never guessed. Returns `None` for kinds with no solid, glyph-free
/// samplable region (`Label`, `Clock`, `NumericDisplay`, `Image`, `VulkanViewport`) and for
/// `StatusIndicator`, whose active-state color token depends on runtime source state that this
/// pure, compiled-data-only engine does not carry — a future wave can extend
/// [`FrameExpectations`] with a resolved active-state token to bring it into scope.
fn chrome_sampling(node: &CompiledNode) -> Option<(String, [u8; 4], Vec<Rect>)> {
    match node.kind {
        CompiledNodeKind::Panel(spec) => {
            let rgba = resolve_color_token(spec.color_token)?;
            let region = inset(node.bounds, PANEL_INSET)?;
            Some((spec.color_token.to_string(), theme_bytes(rgba), vec![region]))
        }
        CompiledNodeKind::Button(spec) => {
            let rgba = resolve_color_token(spec.color_token)?;
            let bands = edge_bands(node.bounds, BUTTON_EDGE_INSET, BUTTON_BAND_HEIGHT)?;
            Some((spec.color_token.to_string(), theme_bytes(rgba), bands.to_vec()))
        }
        CompiledNodeKind::CriticalButton(spec) => {
            let rgba = resolve_color_token(spec.color_token)?;
            let bands = edge_bands(node.bounds, BUTTON_EDGE_INSET, BUTTON_BAND_HEIGHT)?;
            Some((spec.color_token.to_string(), theme_bytes(rgba), bands.to_vec()))
        }
        CompiledNodeKind::TextInput(_) => {
            // The adapter tints an unfocused field from Theme.Colors.Neutral scaled by 0.35
            // (adapters/mdux-vulkan-winit/src/renderer.rs `create_interactive_rect_resources`),
            // not from the node's own `color_token` (that token tints the caret instead).
            let neutral = resolve_color_token("Theme.Colors.Neutral")?;
            let scaled = [neutral[0] * 0.35, neutral[1] * 0.35, neutral[2] * 0.35, neutral[3]];
            let bands = edge_bands(node.bounds, TEXT_INPUT_EDGE_INSET, TEXT_INPUT_BAND_HEIGHT)?;
            Some((
                "Theme.Colors.Neutral*0.35".to_string(),
                theme_bytes(scaled),
                bands.to_vec(),
            ))
        }
        CompiledNodeKind::StatusIndicator(_)
        | CompiledNodeKind::Label(_)
        | CompiledNodeKind::Clock(_)
        | CompiledNodeKind::NumericDisplay(_)
        | CompiledNodeKind::Image(_)
        | CompiledNodeKind::VulkanViewport(_) => None,
    }
}

pub(crate) fn chrome_color_check(
    node: &CompiledNode,
    frame: &FramePixels,
    requirement_id: Option<String>,
) -> Option<CheckResult> {
    let (expected_token, expected_rgba, bands) = chrome_sampling(node)?;

    let mut sample_count: u32 = 0;
    let mut max_channel_delta: u8 = 0;
    let mut measured_rgba = expected_rgba;

    for band in &bands {
        for y in band.y..(band.y + band.height as i32) {
            for x in band.x..(band.x + band.width as i32) {
                let Some(pixel) = frame.pixel(x, y) else {
                    continue;
                };
                sample_count += 1;
                let delta = channelwise_max_delta(pixel, expected_rgba);
                if delta > max_channel_delta {
                    max_channel_delta = delta;
                    measured_rgba = pixel;
                }
            }
        }
    }

    let outcome = if max_channel_delta <= CHROME_TOLERANCE {
        CheckOutcome::Pass
    } else {
        CheckOutcome::Fail
    };

    Some(CheckResult {
        check_id: format!("{}::chrome_color", node.id),
        node_id: node.id.to_string(),
        kind: CheckKind::ChromeColor,
        requirement_id,
        outcome,
        payload: CheckPayload::ChromeColor {
            expected_token,
            expected_rgba,
            measured_rgba,
            max_channel_delta,
            sample_count,
        },
    })
}

pub(crate) fn golden_bounds_check(
    entry: &GoldenReferenceEntry,
    frame: &FramePixels,
    expectations: &FrameExpectations,
    requirement_id: Option<String>,
) -> CheckResult {
    let background = expectations.background_rgba();
    let search_region = expand_and_clamp(entry.bounds, GOLDEN_SEARCH_MARGIN, frame.width, frame.height);

    let mut min_x = None;
    let mut min_y = None;
    let mut max_x = None;
    let mut max_y = None;

    for y in search_region.y..(search_region.y + search_region.height as i32) {
        for x in search_region.x..(search_region.x + search_region.width as i32) {
            let Some(pixel) = frame.pixel(x, y) else {
                continue;
            };
            if !is_ink(pixel, background) {
                continue;
            }
            min_x = Some(min_x.map_or(x, |current: i32| current.min(x)));
            min_y = Some(min_y.map_or(y, |current: i32| current.min(y)));
            max_x = Some(max_x.map_or(x, |current: i32| current.max(x)));
            max_y = Some(max_y.map_or(y, |current: i32| current.max(y)));
        }
    }

    let measured_ink_bounds = match (min_x, min_y, max_x, max_y) {
        (Some(x0), Some(y0), Some(x1), Some(y1)) => Some(Rect {
            x: x0,
            y: y0,
            width: (x1 - x0 + 1) as u32,
            height: (y1 - y0 + 1) as u32,
        }),
        _ => None,
    };

    // No ink found is vacuously contained: an empty set is a subset of any rect. This can
    // legitimately happen for a golden reference whose fill color equals the background.
    let contained = match measured_ink_bounds {
        None => true,
        Some(ink_bounds) => rect_contains(entry.bounds, ink_bounds),
    };

    CheckResult {
        check_id: format!("{}::golden_bounds", entry.node_id),
        node_id: entry.node_id.to_string(),
        kind: CheckKind::GoldenBounds,
        requirement_id,
        outcome: if contained { CheckOutcome::Pass } else { CheckOutcome::Fail },
        payload: CheckPayload::GoldenBounds {
            expected_bounds: entry.bounds,
            measured_ink_bounds,
            contained,
        },
    }
}

pub(crate) fn text_presence_check(
    node: &CompiledNode,
    glyph_count: u32,
    frame: &FramePixels,
    expectations: &FrameExpectations,
    requirement_id: Option<String>,
) -> CheckResult {
    let background = expectations.background_rgba();
    let bounds = node.bounds;

    let mut ink_pixels: u64 = 0;
    for y in bounds.y..(bounds.y + bounds.height as i32) {
        for x in bounds.x..(bounds.x + bounds.width as i32) {
            if let Some(pixel) = frame.pixel(x, y) {
                if is_ink(pixel, background) {
                    ink_pixels += 1;
                }
            }
        }
    }

    let area = u64::from(bounds.width) * u64::from(bounds.height);
    let coverage_ppm = if area == 0 {
        0
    } else {
        ((ink_pixels * 1_000_000) / area) as u32
    };

    let (expected_min_ppm, expected_max_ppm) = crate::coverage_band_for_glyphs(glyph_count, bounds);
    let outcome = if coverage_ppm >= expected_min_ppm && coverage_ppm <= expected_max_ppm {
        CheckOutcome::Pass
    } else {
        CheckOutcome::Fail
    };

    CheckResult {
        check_id: format!("{}::text_presence", node.id),
        node_id: node.id.to_string(),
        kind: CheckKind::TextPresence,
        requirement_id,
        outcome,
        payload: CheckPayload::TextPresence {
            coverage_ppm,
            expected_min_ppm,
            expected_max_ppm,
        },
    }
}

/// Zero ink pixels outside `node`'s own bounds within a margin, except pixels that fall inside
/// another node's bounds (adjacent nodes are legal — ADR-014 already guarantees disjoint bounds,
/// so an ink pixel inside another node's box is that node's own content, not overflow from this
/// one). Algorithm: scan `node.bounds` expanded by [`CONTAINMENT_MARGIN`], skip pixels inside
/// `node.bounds` itself, skip pixels inside any other node's bounds, and count any remaining ink
/// pixel as a containment violation.
pub(crate) fn ink_containment_check(
    node: &CompiledNode,
    all_nodes: &[CompiledNode],
    frame: &FramePixels,
    expectations: &FrameExpectations,
    requirement_id: Option<String>,
) -> CheckResult {
    let background = expectations.background_rgba();
    let region = expand_and_clamp(node.bounds, CONTAINMENT_MARGIN, frame.width, frame.height);

    let mut outside_ink_pixels: u32 = 0;
    for y in region.y..(region.y + region.height as i32) {
        for x in region.x..(region.x + region.width as i32) {
            if point_in_rect(x, y, node.bounds) {
                continue;
            }
            let Some(pixel) = frame.pixel(x, y) else {
                continue;
            };
            if !is_ink(pixel, background) {
                continue;
            }
            let inside_other_node = all_nodes
                .iter()
                .any(|other| other.id != node.id && point_in_rect(x, y, other.bounds));
            if !inside_other_node {
                outside_ink_pixels += 1;
            }
        }
    }

    CheckResult {
        check_id: format!("{}::ink_containment", node.id),
        node_id: node.id.to_string(),
        kind: CheckKind::InkContainment,
        requirement_id,
        outcome: if outside_ink_pixels == 0 {
            CheckOutcome::Pass
        } else {
            CheckOutcome::Fail
        },
        payload: CheckPayload::InkContainment { outside_ink_pixels },
    }
}

pub(crate) fn color_hash_check(
    entry: &GoldenReferenceEntry,
    frame: FramePixels,
    expectations: &FrameExpectations,
    requirement_id: Option<String>,
) -> CheckResult {
    let measured_hex = color_hash(frame, entry.bounds);
    let expected_hex = expectations.color_hash_baseline(entry.node_id).map(str::to_string);

    let outcome = match &expected_hex {
        None => CheckOutcome::NoBaseline,
        Some(expected) if *expected == measured_hex => CheckOutcome::Pass,
        Some(_) => CheckOutcome::Fail,
    };

    CheckResult {
        check_id: format!("{}::color_hash", entry.node_id),
        node_id: entry.node_id.to_string(),
        kind: CheckKind::ColorHash,
        requirement_id,
        outcome,
        payload: CheckPayload::ColorHash {
            expected_hex,
            measured_hex,
        },
    }
}

pub(crate) fn declares_cv_check(entry: &GoldenReferenceEntry, kind: CvCheckKind) -> bool {
    entry.cv_checks.contains(&kind)
}
