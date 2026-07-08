//! Check-engine tests: everything runs on synthetic RGBA buffers built with [`FrameBuilder`] —
//! no GPU anywhere, per ADR-016 §2.

use mdux_ui::{
    ButtonSpec, CompiledNode, CompiledNodeKind, CompiledScreenPackage, CvCheckKind,
    GoldenReferenceEntry, LabelSpec, LayoutKind, LayoutSpec, PanelSpec, Rect, TextInputSpec,
};

use crate::checks::{chrome_color_check, color_hash_check, golden_bounds_check, ink_containment_check, text_presence_check};
use crate::{CheckOutcome, CheckPayload, FrameExpectations, FramePixels, color_hash, coverage_band_for_glyphs, verify_frame};

/// A synthetic RGBA8 frame builder: fills a background, then lets tests stamp solid rects on top
/// — exactly the pixel shapes the check engine consumes, with no renderer involved.
struct FrameBuilder {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl FrameBuilder {
    fn new(width: u32, height: u32, background: [u8; 4]) -> Self {
        let mut pixels = Vec::with_capacity((width as usize) * (height as usize) * 4);
        for _ in 0..(width * height) {
            pixels.extend_from_slice(&background);
        }
        Self { width, height, pixels }
    }

    fn fill_rect(&mut self, rect: Rect, rgba: [u8; 4]) -> &mut Self {
        for y in rect.y..(rect.y + rect.height as i32) {
            for x in rect.x..(rect.x + rect.width as i32) {
                if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
                    continue;
                }
                let index = ((y as u32 * self.width + x as u32) as usize) * 4;
                self.pixels[index..index + 4].copy_from_slice(&rgba);
            }
        }
        self
    }

    fn frame(&self) -> FramePixels<'_> {
        FramePixels {
            width: self.width,
            height: self.height,
            rgba: &self.pixels,
        }
    }
}

const BACKGROUND: [u8; 4] = [10, 10, 10, 255];

// Theme.Colors.PrimaryAction = [0.16, 0.44, 0.86, 1.0] -> round(255 * component):
// 0.16*255=40.8 -> 41, 0.44*255=112.2 -> 112, 0.86*255=219.3 -> 219, alpha -> 255.
const PRIMARY_ACTION_RGBA: [u8; 4] = [41, 112, 219, 255];

// Theme.Colors.Neutral = [0.62, 0.66, 0.70, 1.0], scaled by 0.35 (unfocused TextInput field,
// adapters/mdux-vulkan-winit/src/renderer.rs `create_interactive_rect_resources`):
// 0.62*0.35*255=55.335 -> 55, 0.66*0.35*255=58.905 -> 59, 0.70*0.35*255=62.475 -> 62.
const TEXT_INPUT_FIELD_RGBA: [u8; 4] = [55, 59, 62, 255];

fn button_node(bounds: Rect) -> CompiledNode {
    CompiledNode {
        id: "ack-button",
        bounds,
        kind: CompiledNodeKind::Button(ButtonSpec {
            text_key: "STR-NS-ACK",
            color_token: "Theme.Colors.PrimaryAction",
            source: "ACK_BUTTON",
            requirement_id: Some("REQ-NS-004"),
        }),
    }
}

fn text_input_node(bounds: Rect) -> CompiledNode {
    CompiledNode {
        id: "patient-id-input",
        bounds,
        kind: CompiledNodeKind::TextInput(TextInputSpec {
            source: "PATIENT_ID",
            max_length: 16,
            glyph_set_id: "SET-ASCII-TEXT",
            color_token: "Theme.Colors.Title",
            requirement_id: Some("REQ-NS-005"),
        }),
    }
}

const BUTTON_BOUNDS: Rect = Rect { x: 20, y: 20, width: 200, height: 64 };

// ---- ChromeColor -----------------------------------------------------------------------------

#[test]
fn chrome_color_passes_when_face_matches_theme_byte_exactly() {
    let mut builder = FrameBuilder::new(300, 200, BACKGROUND);
    builder.fill_rect(BUTTON_BOUNDS, PRIMARY_ACTION_RGBA);
    let node = button_node(BUTTON_BOUNDS);

    let result = chrome_color_check(&node, &builder.frame(), Some("REQ-NS-004".to_string()))
        .expect("button has a chrome-samplable region");

    assert_eq!(result.outcome, CheckOutcome::Pass);
    match result.payload {
        CheckPayload::ChromeColor {
            expected_rgba,
            max_channel_delta,
            sample_count,
            ..
        } => {
            assert_eq!(expected_rgba, PRIMARY_ACTION_RGBA);
            assert_eq!(max_channel_delta, 0);
            assert!(sample_count > 0);
        }
        other => panic!("expected ChromeColor payload, got {other:?}"),
    }
}

#[test]
fn chrome_color_tolerance_edge_off_by_one_still_passes() {
    let mut builder = FrameBuilder::new(300, 200, BACKGROUND);
    let off_by_one = [
        PRIMARY_ACTION_RGBA[0] + 1,
        PRIMARY_ACTION_RGBA[1],
        PRIMARY_ACTION_RGBA[2],
        PRIMARY_ACTION_RGBA[3],
    ];
    builder.fill_rect(BUTTON_BOUNDS, off_by_one);
    let node = button_node(BUTTON_BOUNDS);

    let result = chrome_color_check(&node, &builder.frame(), None).expect("chrome region");
    assert_eq!(result.outcome, CheckOutcome::Pass);
    match result.payload {
        CheckPayload::ChromeColor { max_channel_delta, .. } => assert_eq!(max_channel_delta, 1),
        other => panic!("expected ChromeColor payload, got {other:?}"),
    }
}

#[test]
fn chrome_color_fails_when_no_pixel_could_be_sampled() {
    // A 1x1 frame: every coordinate in the button's chrome bands falls outside width/height, so
    // frame.pixel() returns None everywhere and sample_count stays 0. This must fail, not read
    // as a Pass with max_channel_delta still at its untouched initial 0.
    let builder = FrameBuilder::new(1, 1, BACKGROUND);
    let node = button_node(BUTTON_BOUNDS);

    let result = chrome_color_check(&node, &builder.frame(), None).expect("chrome region");

    assert_eq!(result.outcome, CheckOutcome::Fail);
    match result.payload {
        CheckPayload::ChromeColor { sample_count, max_channel_delta, .. } => {
            assert_eq!(sample_count, 0);
            assert_eq!(max_channel_delta, 0);
        }
        other => panic!("expected ChromeColor payload, got {other:?}"),
    }
}

#[test]
fn chrome_color_fails_when_off_by_two_channels() {
    let mut builder = FrameBuilder::new(300, 200, BACKGROUND);
    let off_by_two = [
        PRIMARY_ACTION_RGBA[0] + 2,
        PRIMARY_ACTION_RGBA[1],
        PRIMARY_ACTION_RGBA[2],
        PRIMARY_ACTION_RGBA[3],
    ];
    builder.fill_rect(BUTTON_BOUNDS, off_by_two);
    let node = button_node(BUTTON_BOUNDS);

    let result = chrome_color_check(&node, &builder.frame(), None).expect("chrome region");
    assert_eq!(result.outcome, CheckOutcome::Fail);
    match result.payload {
        CheckPayload::ChromeColor { max_channel_delta, .. } => assert_eq!(max_channel_delta, 2),
        other => panic!("expected ChromeColor payload, got {other:?}"),
    }
}

#[test]
fn chrome_color_text_input_expects_neutral_scaled_field_not_its_own_token() {
    let bounds = Rect { x: 20, y: 20, width: 200, height: 48 };
    let mut builder = FrameBuilder::new(300, 200, BACKGROUND);
    builder.fill_rect(bounds, TEXT_INPUT_FIELD_RGBA);
    let node = text_input_node(bounds);

    let result = chrome_color_check(&node, &builder.frame(), None).expect("text input has a field band");
    assert_eq!(result.outcome, CheckOutcome::Pass);
    match result.payload {
        CheckPayload::ChromeColor {
            expected_token,
            expected_rgba,
            ..
        } => {
            assert_eq!(expected_token, "Theme.Colors.Neutral*0.35");
            assert_eq!(expected_rgba, TEXT_INPUT_FIELD_RGBA);
        }
        other => panic!("expected ChromeColor payload, got {other:?}"),
    }
}

#[test]
fn chrome_color_none_for_kinds_without_a_glyph_free_region() {
    let node = CompiledNode {
        id: "title",
        bounds: Rect { x: 0, y: 0, width: 100, height: 30 },
        kind: CompiledNodeKind::Label(LabelSpec {
            text_key: "STR-TITLE",
            color_token: "Theme.Colors.Title",
        }),
    };
    let builder = FrameBuilder::new(100, 30, BACKGROUND);
    assert!(chrome_color_check(&node, &builder.frame(), None).is_none());
}

// ---- GoldenBounds -----------------------------------------------------------------------------

const GOLDEN_BOUNDS: Rect = Rect { x: 50, y: 50, width: 40, height: 20 };

#[test]
fn golden_bounds_passes_when_ink_stays_inside_entry_bounds() {
    let mut builder = FrameBuilder::new(200, 150, BACKGROUND);
    builder.fill_rect(GOLDEN_BOUNDS, [200, 200, 200, 255]);
    let entry = GoldenReferenceEntry {
        node_id: "sedation-index",
        bounds: GOLDEN_BOUNDS,
        text_key: None,
        color_token: None,
        cv_checks: &[CvCheckKind::Bounds],
    };
    let expectations = FrameExpectations::new(BACKGROUND);

    let result = golden_bounds_check(&entry, &builder.frame(), &expectations, None);
    assert_eq!(result.outcome, CheckOutcome::Pass);
    match result.payload {
        CheckPayload::GoldenBounds { contained, measured_ink_bounds, .. } => {
            assert!(contained);
            assert_eq!(measured_ink_bounds, Some(GOLDEN_BOUNDS));
        }
        other => panic!("expected GoldenBounds payload, got {other:?}"),
    }
}

#[test]
fn golden_bounds_fails_when_ink_shifts_outside_entry_bounds() {
    let mut builder = FrameBuilder::new(200, 150, BACKGROUND);
    // Ink shifted 10px right of the declared bounds: still inside the search margin (so it is
    // detected, not cropped away), but no longer contained.
    let shifted = Rect { x: GOLDEN_BOUNDS.x + 10, ..GOLDEN_BOUNDS };
    builder.fill_rect(shifted, [200, 200, 200, 255]);
    let entry = GoldenReferenceEntry {
        node_id: "sedation-index",
        bounds: GOLDEN_BOUNDS,
        text_key: None,
        color_token: None,
        cv_checks: &[CvCheckKind::Bounds],
    };
    let expectations = FrameExpectations::new(BACKGROUND);

    let result = golden_bounds_check(&entry, &builder.frame(), &expectations, None);
    assert_eq!(result.outcome, CheckOutcome::Fail);
    match result.payload {
        CheckPayload::GoldenBounds { contained, .. } => assert!(!contained),
        other => panic!("expected GoldenBounds payload, got {other:?}"),
    }
}

// ---- TextPresence -----------------------------------------------------------------------------

const TEXT_BOUNDS: Rect = Rect { x: 0, y: 0, width: 100, height: 20 };

#[test]
fn text_presence_passes_within_the_expected_coverage_band() {
    let mut builder = FrameBuilder::new(100, 20, [255, 255, 255, 255]);
    // 5x10 = 50 ink pixels inside a 2000px area -> 25,000 ppm, inside the band for 5 glyphs.
    builder.fill_rect(Rect { x: 10, y: 5, width: 5, height: 10 }, [0, 0, 0, 255]);
    let node = CompiledNode {
        id: "clock",
        bounds: TEXT_BOUNDS,
        kind: CompiledNodeKind::Label(LabelSpec {
            text_key: "STR-CLOCK",
            color_token: "Theme.Colors.Title",
        }),
    };
    let expectations = FrameExpectations::new([255, 255, 255, 255]);

    let (min_ppm, max_ppm) = coverage_band_for_glyphs(5, TEXT_BOUNDS);
    let result = text_presence_check(&node, 5, &builder.frame(), &expectations, None);
    assert_eq!(result.outcome, CheckOutcome::Pass);
    match result.payload {
        CheckPayload::TextPresence {
            coverage_ppm,
            expected_min_ppm,
            expected_max_ppm,
        } => {
            assert_eq!(coverage_ppm, 25_000);
            assert_eq!(expected_min_ppm, min_ppm);
            assert_eq!(expected_max_ppm, max_ppm);
        }
        other => panic!("expected TextPresence payload, got {other:?}"),
    }
}

#[test]
fn text_presence_fails_on_a_blank_region() {
    let builder = FrameBuilder::new(100, 20, [255, 255, 255, 255]);
    let node = CompiledNode {
        id: "clock",
        bounds: TEXT_BOUNDS,
        kind: CompiledNodeKind::Label(LabelSpec {
            text_key: "STR-CLOCK",
            color_token: "Theme.Colors.Title",
        }),
    };
    let expectations = FrameExpectations::new([255, 255, 255, 255]);

    let result = text_presence_check(&node, 5, &builder.frame(), &expectations, None);
    assert_eq!(result.outcome, CheckOutcome::Fail);
    match result.payload {
        CheckPayload::TextPresence { coverage_ppm, .. } => assert_eq!(coverage_ppm, 0),
        other => panic!("expected TextPresence payload, got {other:?}"),
    }
}

#[test]
fn text_presence_fails_on_over_coverage() {
    let mut builder = FrameBuilder::new(100, 20, [255, 255, 255, 255]);
    builder.fill_rect(TEXT_BOUNDS, [0, 0, 0, 255]);
    let node = CompiledNode {
        id: "clock",
        bounds: TEXT_BOUNDS,
        kind: CompiledNodeKind::Label(LabelSpec {
            text_key: "STR-CLOCK",
            color_token: "Theme.Colors.Title",
        }),
    };
    let expectations = FrameExpectations::new([255, 255, 255, 255]);

    let result = text_presence_check(&node, 5, &builder.frame(), &expectations, None);
    assert_eq!(result.outcome, CheckOutcome::Fail);
    match result.payload {
        CheckPayload::TextPresence { coverage_ppm, .. } => assert_eq!(coverage_ppm, 1_000_000),
        other => panic!("expected TextPresence payload, got {other:?}"),
    }
}

#[test]
fn coverage_band_for_glyphs_is_zero_width_with_no_glyphs_or_area() {
    assert_eq!(coverage_band_for_glyphs(0, TEXT_BOUNDS), (0, 0));
    assert_eq!(coverage_band_for_glyphs(5, Rect { x: 0, y: 0, width: 0, height: 20 }), (0, 0));
}

// ---- InkContainment ---------------------------------------------------------------------------

#[test]
fn ink_containment_passes_when_ink_stays_inside_the_node() {
    let node_a = CompiledNode {
        id: "node-a",
        bounds: Rect { x: 30, y: 30, width: 50, height: 50 },
        kind: CompiledNodeKind::Panel(PanelSpec { color_token: "Theme.Colors.Neutral" }),
    };
    let mut builder = FrameBuilder::new(200, 100, BACKGROUND);
    builder.fill_rect(node_a.bounds, [200, 200, 200, 255]);
    let expectations = FrameExpectations::new(BACKGROUND);

    let result = ink_containment_check(&node_a, std::slice::from_ref(&node_a), &builder.frame(), &expectations, None);
    assert_eq!(result.outcome, CheckOutcome::Pass);
    match result.payload {
        CheckPayload::InkContainment { outside_ink_pixels } => assert_eq!(outside_ink_pixels, 0),
        other => panic!("expected InkContainment payload, got {other:?}"),
    }
}

#[test]
fn ink_containment_fails_on_out_of_bounds_ink() {
    let node_a = CompiledNode {
        id: "node-a",
        bounds: Rect { x: 30, y: 30, width: 50, height: 50 },
        kind: CompiledNodeKind::Panel(PanelSpec { color_token: "Theme.Colors.Neutral" }),
    };
    // Unrelated, far-away node so the leaked ink pixel below is not legitimized as belonging to
    // another node.
    let node_far = CompiledNode {
        id: "node-far",
        bounds: Rect { x: 150, y: 0, width: 20, height: 20 },
        kind: CompiledNodeKind::Panel(PanelSpec { color_token: "Theme.Colors.Neutral" }),
    };
    let mut builder = FrameBuilder::new(200, 100, BACKGROUND);
    builder.fill_rect(node_a.bounds, [200, 200, 200, 255]);
    // One leaked ink pixel just outside node_a's left edge, still inside its containment margin.
    builder.fill_rect(Rect { x: 25, y: 40, width: 1, height: 1 }, [200, 200, 200, 255]);
    let expectations = FrameExpectations::new(BACKGROUND);
    let nodes = [node_a.clone(), node_far.clone()];

    let result = ink_containment_check(&node_a, &nodes, &builder.frame(), &expectations, None);
    assert_eq!(result.outcome, CheckOutcome::Fail);
    match result.payload {
        CheckPayload::InkContainment { outside_ink_pixels } => assert_eq!(outside_ink_pixels, 1),
        other => panic!("expected InkContainment payload, got {other:?}"),
    }
}

#[test]
fn ink_containment_allows_ink_that_belongs_to_an_adjacent_node() {
    let node_a = CompiledNode {
        id: "node-a",
        bounds: Rect { x: 30, y: 30, width: 50, height: 50 },
        kind: CompiledNodeKind::Panel(PanelSpec { color_token: "Theme.Colors.Neutral" }),
    };
    // Node C sits directly against node A's right edge (statically disjoint, per ADR-014).
    let node_c = CompiledNode {
        id: "node-c",
        bounds: Rect { x: 80, y: 30, width: 50, height: 50 },
        kind: CompiledNodeKind::Panel(PanelSpec { color_token: "Theme.Colors.Neutral" }),
    };
    let mut builder = FrameBuilder::new(200, 100, BACKGROUND);
    builder.fill_rect(node_a.bounds, [200, 200, 200, 255]);
    // Ink belonging to node_c, inside node_a's containment margin (margin reaches x in [22,88)).
    builder.fill_rect(Rect { x: 82, y: 32, width: 2, height: 2 }, [200, 200, 200, 255]);
    let expectations = FrameExpectations::new(BACKGROUND);
    let nodes = [node_a.clone(), node_c.clone()];

    let result = ink_containment_check(&node_a, &nodes, &builder.frame(), &expectations, None);
    assert_eq!(result.outcome, CheckOutcome::Pass);
    match result.payload {
        CheckPayload::InkContainment { outside_ink_pixels } => assert_eq!(outside_ink_pixels, 0),
        other => panic!("expected InkContainment payload, got {other:?}"),
    }
}

// ---- ColorHash --------------------------------------------------------------------------------

const HASH_BOUNDS: Rect = Rect { x: 0, y: 0, width: 2, height: 2 };
// sha256([10,20,30,255] repeated 4 times) — cross-checked independently with Python's hashlib.
const HASH_FIXTURE_HEX: &str = "850ddc69d6a093082c28269fd232b7543f8133b013b4820a4f15ebfe769d18d4";

#[test]
fn color_hash_matches_an_independently_computed_known_answer() {
    let mut builder = FrameBuilder::new(2, 2, [0, 0, 0, 255]);
    builder.fill_rect(HASH_BOUNDS, [10, 20, 30, 255]);
    assert_eq!(color_hash(builder.frame(), HASH_BOUNDS), HASH_FIXTURE_HEX);
}

#[test]
fn color_hash_check_passes_when_it_matches_the_committed_baseline() {
    let mut builder = FrameBuilder::new(2, 2, [0, 0, 0, 255]);
    builder.fill_rect(HASH_BOUNDS, [10, 20, 30, 255]);
    let entry = GoldenReferenceEntry {
        node_id: "sedation-index",
        bounds: HASH_BOUNDS,
        text_key: None,
        color_token: None,
        cv_checks: &[CvCheckKind::ColorHash],
    };
    let expectations = FrameExpectations::new([0, 0, 0, 255])
        .with_color_hash_baseline("sedation-index", HASH_FIXTURE_HEX);

    let result = color_hash_check(&entry, builder.frame(), &expectations, None);
    assert_eq!(result.outcome, CheckOutcome::Pass);
    match result.payload {
        CheckPayload::ColorHash { expected_hex, measured_hex } => {
            assert_eq!(expected_hex.as_deref(), Some(HASH_FIXTURE_HEX));
            assert_eq!(measured_hex, HASH_FIXTURE_HEX);
        }
        other => panic!("expected ColorHash payload, got {other:?}"),
    }
}

#[test]
fn color_hash_check_fails_on_mismatch() {
    let mut builder = FrameBuilder::new(2, 2, [0, 0, 0, 255]);
    builder.fill_rect(HASH_BOUNDS, [11, 20, 30, 255]);
    let entry = GoldenReferenceEntry {
        node_id: "sedation-index",
        bounds: HASH_BOUNDS,
        text_key: None,
        color_token: None,
        cv_checks: &[CvCheckKind::ColorHash],
    };
    let expectations = FrameExpectations::new([0, 0, 0, 255])
        .with_color_hash_baseline("sedation-index", HASH_FIXTURE_HEX);

    let result = color_hash_check(&entry, builder.frame(), &expectations, None);
    assert_eq!(result.outcome, CheckOutcome::Fail);
}

#[test]
fn color_hash_check_reports_no_baseline_and_it_is_never_a_pass() {
    let mut builder = FrameBuilder::new(2, 2, [0, 0, 0, 255]);
    builder.fill_rect(HASH_BOUNDS, [10, 20, 30, 255]);
    let entry = GoldenReferenceEntry {
        node_id: "sedation-index",
        bounds: HASH_BOUNDS,
        text_key: None,
        color_token: None,
        cv_checks: &[CvCheckKind::ColorHash],
    };
    let expectations = FrameExpectations::new([0, 0, 0, 255]);

    let result = color_hash_check(&entry, builder.frame(), &expectations, None);
    assert_eq!(result.outcome, CheckOutcome::NoBaseline);
    assert!(!result.outcome.is_pass());
    match result.payload {
        CheckPayload::ColorHash { expected_hex, .. } => assert_eq!(expected_hex, None),
        other => panic!("expected ColorHash payload, got {other:?}"),
    }
}

// ---- FramePixels --------------------------------------------------------------------------------

#[test]
fn frame_pixels_returns_none_outside_bounds() {
    let builder = FrameBuilder::new(4, 4, BACKGROUND);
    let frame = builder.frame();
    assert_eq!(frame.pixel(0, 0), Some(BACKGROUND));
    assert_eq!(frame.pixel(-1, 0), None);
    assert_eq!(frame.pixel(0, -1), None);
    assert_eq!(frame.pixel(4, 0), None);
    assert_eq!(frame.pixel(0, 4), None);
}

// ---- verify_frame integration ------------------------------------------------------------------

// `CompiledScreenPackage`'s fields are `&'static`, matching the real compiler output (screens
// are `&'static` data baked into application binaries) — as in `mdux-ui/src/lib.rs`'s own tests,
// the fixture is a `const` of inline literals rather than built from the non-`const`
// `panel_node`/`button_node` helpers used elsewhere in this file.
const INTEGRATION_SCREEN: CompiledScreenPackage = CompiledScreenPackage {
    screen_id: "TestScreen",
    layout: LayoutSpec { kind: LayoutKind::Vertical, spacing: 8, padding: 16 },
    nodes: &[
        CompiledNode {
            id: "topbar-background",
            bounds: Rect { x: 0, y: 0, width: 300, height: 200 },
            kind: CompiledNodeKind::Panel(PanelSpec {
                color_token: "Theme.Colors.TopbarBackground",
            }),
        },
        CompiledNode {
            id: "ack-button",
            bounds: BUTTON_BOUNDS,
            kind: CompiledNodeKind::Button(ButtonSpec {
                text_key: "STR-NS-ACK",
                color_token: "Theme.Colors.PrimaryAction",
                source: "ACK_BUTTON",
                requirement_id: Some("REQ-NS-004"),
            }),
        },
    ],
    golden_references: &[GoldenReferenceEntry {
        node_id: "ack-button",
        bounds: BUTTON_BOUNDS,
        text_key: Some("STR-NS-ACK"),
        color_token: Some("Theme.Colors.PrimaryAction"),
        cv_checks: &[CvCheckKind::Bounds, CvCheckKind::ColorHash],
    }],
};

#[test]
fn verify_frame_skips_ink_containment_for_panels_and_runs_every_other_check() {
    // Theme.Colors.TopbarBackground = [0.82, 0.84, 0.86, 1.0] -> round(255 * component):
    // 0.82*255=209.1 -> 209, 0.84*255=214.2 -> 214, 0.86*255=219.3 -> 219.
    let mut builder = FrameBuilder::new(300, 200, [209, 214, 219, 255]);
    builder.fill_rect(BUTTON_BOUNDS, PRIMARY_ACTION_RGBA);

    let expectations = FrameExpectations::new([209, 214, 219, 255])
        .with_color_hash_baseline("ack-button", color_hash(builder.frame(), BUTTON_BOUNDS));

    let results = verify_frame(&INTEGRATION_SCREEN, builder.frame(), &expectations);

    let panel_ink_containment = results
        .iter()
        .find(|result| result.node_id == "topbar-background" && result.kind == crate::CheckKind::InkContainment);
    assert!(panel_ink_containment.is_none(), "Panel is exempt from InkContainment");

    let button_ink_containment = results
        .iter()
        .find(|result| result.node_id == "ack-button" && result.kind == crate::CheckKind::InkContainment)
        .expect("non-panel nodes get InkContainment");
    assert_eq!(button_ink_containment.outcome, CheckOutcome::Pass);

    let button_chrome = results
        .iter()
        .find(|result| result.node_id == "ack-button" && result.kind == crate::CheckKind::ChromeColor)
        .expect("button has ChromeColor");
    assert_eq!(button_chrome.requirement_id.as_deref(), Some("REQ-NS-004"));
    assert_eq!(button_chrome.outcome, CheckOutcome::Pass);

    let golden_bounds = results
        .iter()
        .find(|result| result.node_id == "ack-button" && result.kind == crate::CheckKind::GoldenBounds)
        .expect("golden reference declares Bounds");
    assert_eq!(golden_bounds.outcome, CheckOutcome::Pass);

    let hash_check = results
        .iter()
        .find(|result| result.node_id == "ack-button" && result.kind == crate::CheckKind::ColorHash)
        .expect("golden reference declares ColorHash");
    assert_eq!(hash_check.outcome, CheckOutcome::Pass);
}
