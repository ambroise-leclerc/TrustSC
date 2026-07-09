//! ADR-016 `--verify-ui` run mode (wave W6, epic #91): wires the offscreen render path (§1), the
//! pure check engine `mdux-ui-verify` (§2/§3), and the scenario plane (§4) into [`crate::App`] as
//! a verification run mode symmetric with `--headless-smoke`. Per locale: render the compiled
//! screen's static truth offscreen, run every `mdux-ui-verify` check against it, replay every
//! registered scenario (capturing and re-checking each named step), and write the schema v1
//! evidence report (§5) plus PPM captures under the given directory. Returns `Err` (a non-zero
//! process exit through `main`'s `Result`) if any check failed or any scenario step failed —
//! only after every report has been written, so CI can still upload the evidence on failure.

use std::{fs, path::Path};

use mdux::input::FrameEvents;
use mdux::realtime::{FrameInputs, ScreenBindings};
use mdux::screen_text::ScreenTextLayout;
use mdux::verify_scenario::{ScenarioScript, ScenarioTrace, run_scenario};
use mdux_ui_verify::{
    CheckOutcome, CheckPayload, CheckResult, FrameExpectations, FramePixels, ScenarioTraceRow,
    TOOL_NAME, TOOL_VERSION, VerificationReport, emit_report_json, sha256_hex, verify_frame,
};

use crate::offscreen::OffscreenRenderer;
use crate::renderer::{BoxError, InteractionSnapshot, OFFSCREEN_PIXEL_FORMAT_NAME, WallClock, clear_color_bytes};
use crate::App;

/// The fixed clock the base (non-scenario) capture pins for `Clock` nodes — determinism the same
/// way a scenario's own `clock` field pins its captures (ADR-016 §1/§4): identical screen +
/// locale + inputs + backend always renders identical bytes.
const VERIFY_BASE_CLOCK: WallClock = WallClock {
    year: 2026,
    month: 1,
    day: 1,
    hours: 12,
    minutes: 0,
    seconds: 0,
};

/// Which locales `--verify-ui` verifies. `Default` preserves existing single-locale behavior for
/// callers that never touch this flag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocaleSelection {
    /// The single locale [`crate::App::with_locale`] configured (or `en-US` if never called).
    Default,
    /// Every locale the default standard text package declares
    /// ([`mdux::TextPackage::locales`]) — added automatically as new locales are baked in.
    All,
    /// An explicit, caller-supplied list.
    List(Vec<String>),
}

impl Default for LocaleSelection {
    fn default() -> Self {
        LocaleSelection::Default
    }
}

pub(crate) type ScenarioClosures = (
    Box<dyn FnMut(&mut FrameEvents, &mut FrameInputs)>,
    Box<dyn FnMut(&mut FrameInputs)>,
);
/// A factory [`crate::App::with_scenarios`] calls once per scenario replay to build a *fresh*
/// pair of closures — see that method's doc comment for why freshness matters.
pub(crate) type ScenarioLogicFactory = Box<dyn Fn() -> ScenarioClosures>;

/// Drives the whole `--verify-ui` run for one [`App`]: resolves locales, then per locale renders
/// the base capture, runs the check suite, replays every applicable scenario, and writes the
/// report. Consumes `app` — verification is a terminal run mode, like `--headless-smoke`.
pub(crate) fn run_verify(
    app: App,
    dir: &Path,
    locales: &LocaleSelection,
    scenario_filter: Option<&str>,
) -> Result<(), BoxError> {
    let App {
        framework,
        screen,
        locale: default_locale,
        scenarios,
        scenario_logic,
        ..
    } = app;

    let scenarios_to_run: Vec<&ScenarioScript> = match scenario_filter {
        Some(id) => {
            let filtered: Vec<&ScenarioScript> =
                scenarios.iter().filter(|scenario| scenario.id == id).collect();
            if filtered.is_empty() {
                return Err(format!(
                    "--scenario={id} does not match any scenario registered with App::with_scenarios"
                )
                .into());
            }
            filtered
        }
        None => scenarios.iter().collect(),
    };

    let standard_package = mdux::default_standard_text_package()?;
    let display_packages = mdux::default_display_text_packages()?;
    let image_packages = mdux::default_image_packages()?;

    let resolved_locales: Vec<String> = match locales {
        LocaleSelection::Default => vec![default_locale.clone()],
        LocaleSelection::All => standard_package.locales(),
        LocaleSelection::List(list) => list.clone(),
    };
    if resolved_locales.is_empty() {
        return Err(
            "no locales resolved to verify — the standard text package declares none".into(),
        );
    }

    let device = framework.device().clone();
    let trace_rows_gov = framework.trace_rows();
    let config = framework.ui_runtime().config().clone();
    let app_name = framework.identity().name.clone();

    let screen_dir = dir.join(screen.screen_id);
    let mut any_failure = false;

    for locale in &resolved_locales {
        let layout = ScreenTextLayout::from_screen(screen, standard_package.clone(), locale)?;
        let bindings = ScreenBindings::from_screen(
            screen,
            standard_package.clone(),
            display_packages.clone(),
            &image_packages,
            locale,
        )?;
        let frame_inputs_template = FrameInputs::from_bindings(&bindings)?;

        // Static text nodes only (Label/Button/CriticalButton): dynamic content (Clock,
        // NumericDisplay, StatusIndicator, TextInput) has no compile-time-known glyph count, so
        // TextPresence is skipped for them here — GoldenBounds/ChromeColor/InkContainment still
        // fully cover their geometry and color.
        let mut glyph_counts: Vec<(&'static str, u32)> = Vec::new();
        for node in screen.nodes {
            if node.kind.text_key().is_some() {
                if let Some(run) = layout.find_run(node.id) {
                    glyph_counts.push((node.id, run.commands.len() as u32));
                }
            }
        }

        let mut renderer =
            OffscreenRenderer::new(&app_name, layout, bindings, config.width, config.height)?;
        let device_name = renderer.device_name().to_string();
        let backend_id = normalize_backend_id(&device_name);

        let locale_dir = screen_dir.join(locale);
        fs::create_dir_all(&locale_dir)?;

        let base_inputs = frame_inputs_template.clone();
        renderer.draw_frame(&base_inputs, VERIFY_BASE_CLOCK, InteractionSnapshot::default())?;
        let base_captured = renderer.read_pixels()?;

        let screenshot_bytes =
            encode_ppm(base_captured.width, base_captured.height, &base_captured.rgba);
        fs::write(locale_dir.join("screenshot.ppm"), &screenshot_bytes)?;
        let screenshot_sha256 = sha256_hex(&screenshot_bytes);

        // Tier 2 (ADR-016 §3): baselines are lavapipe-only. On any other backend the ColorHash
        // check reports NoBaseline, never a pass; here that also means we never load (or
        // bootstrap) a baseline file for a backend it was never meant to describe.
        let baseline_path = screen_dir.join("baselines").join("lavapipe").join(format!("{locale}.txt"));
        let baselines = if backend_id == "lavapipe" {
            load_baselines(&baseline_path)?
        } else {
            Vec::new()
        };
        let baseline_already_existed = baseline_path.exists();

        let mut expectations = FrameExpectations::new(clear_color_bytes());
        for (node_id, count) in &glyph_counts {
            expectations = expectations.with_glyph_count(*node_id, *count);
        }
        for (node_id, hex) in &baselines {
            expectations = expectations.with_color_hash_baseline(node_id.clone(), hex.clone());
        }

        let base_pixels = FramePixels {
            width: base_captured.width,
            height: base_captured.height,
            rgba: &base_captured.rgba,
        };
        let mut checks: Vec<CheckResult> = verify_frame(screen, base_pixels, &expectations);

        // Bootstrap: the very first lavapipe run for this (screen, locale) has no committed
        // baseline, so every ColorHash check above reported NoBaseline (never a pass). Writing
        // one now from what was just measured is what "self-bootstrapping" means here — a human
        // still reviews and commits the file; this only ever happens when none exists yet.
        let mut bootstrapped_this_run = false;
        if backend_id == "lavapipe" && !baseline_already_existed {
            bootstrapped_this_run = bootstrap_baselines(&baseline_path, &checks)?;
            if bootstrapped_this_run {
                println!(
                    "verify-ui: bootstrapped lavapipe ColorHash baselines for screen={} locale={} at {}",
                    screen.screen_id,
                    locale,
                    baseline_path.display()
                );
            }
        }

        let mut scenario_traces: Vec<ScenarioTraceRow> = Vec::new();
        if let Some(factory) = &scenario_logic {
            for scenario in scenarios_to_run.iter().copied() {
                let (mut input_closure, mut realtime_closure) = factory();
                let mut events = FrameEvents::new();
                let mut scenario_inputs = frame_inputs_template.clone();
                let scenario_clock = WallClock {
                    year: scenario.clock.year,
                    month: scenario.clock.month,
                    day: scenario.clock.day,
                    hours: scenario.clock.hours,
                    minutes: scenario.clock.minutes,
                    seconds: scenario.clock.seconds,
                };

                let mut capture_error: Option<BoxError> = None;
                let trace: ScenarioTrace = run_scenario(
                    scenario,
                    &mut events,
                    &mut scenario_inputs,
                    &mut *input_closure,
                    &mut *realtime_closure,
                    |label, frame_inputs| {
                        if capture_error.is_some() {
                            return;
                        }
                        let outcome = (|| -> Result<(), BoxError> {
                            renderer.draw_frame(
                                frame_inputs,
                                scenario_clock,
                                InteractionSnapshot::default(),
                            )?;
                            let captured = renderer.read_pixels()?;
                            let step_bytes =
                                encode_ppm(captured.width, captured.height, &captured.rgba);
                            fs::write(
                                locale_dir.join(format!("step-{}-{label}.ppm", scenario.id)),
                                &step_bytes,
                            )?;
                            let step_pixels = FramePixels {
                                width: captured.width,
                                height: captured.height,
                                rgba: &captured.rgba,
                            };
                            let mut step_checks = verify_frame(screen, step_pixels, &expectations);
                            for check in &mut step_checks {
                                check.check_id =
                                    format!("{}::{label}::{}", scenario.id, check.check_id);
                            }
                            checks.extend(step_checks);
                            Ok(())
                        })();
                        if let Err(error) = outcome {
                            capture_error = Some(error);
                        }
                    },
                );

                if let Some(error) = capture_error {
                    return Err(error);
                }

                for step in &trace.steps {
                    scenario_traces.push(ScenarioTraceRow {
                        scenario_id: trace.scenario_id.to_string(),
                        step_index: step.index as u32,
                        description: step.description.clone(),
                        expected: step.expected.clone().unwrap_or_default(),
                        observed: step.observed.clone().unwrap_or_default(),
                        passed: step.passed,
                    });
                }
                if !trace.passed {
                    any_failure = true;
                }
            }
        }

        let report_trace_rows: Vec<mdux_ui_verify::TraceRow> = trace_rows_gov
            .iter()
            .map(|gov_row| {
                let check_ids: Vec<String> = checks
                    .iter()
                    .filter(|check| check.requirement_id.as_deref() == Some(gov_row.requirement_id.as_str()))
                    .map(|check| check.check_id.clone())
                    .collect();
                mdux_ui_verify::TraceRow {
                    requirement_id: gov_row.requirement_id.clone(),
                    verification_ids: gov_row.verification_ids.clone(),
                    check_ids,
                }
            })
            .collect();

        let failed_checks = checks.iter().filter(|check| check.outcome == CheckOutcome::Fail).count();
        if failed_checks > 0 {
            any_failure = true;
        }

        println!(
            "verify-ui screen={} locale={} backend={} checks={} failed={} scenarios={} bootstrapped_baselines={}",
            screen.screen_id,
            locale,
            backend_id,
            checks.len(),
            failed_checks,
            scenarios_to_run.len(),
            bootstrapped_this_run
        );

        let report = VerificationReport {
            tool_name: TOOL_NAME.to_string(),
            tool_version: TOOL_VERSION.to_string(),
            software_item: device.software_item.clone(),
            safety_class: device.safety_class.to_string(),
            screen_id: screen.screen_id.to_string(),
            locale: locale.clone(),
            surface_width: base_captured.width,
            surface_height: base_captured.height,
            backend_id,
            device_name,
            pixel_format: OFFSCREEN_PIXEL_FORMAT_NAME.to_string(),
            clock: wall_clock_iso(VERIFY_BASE_CLOCK),
            screenshot_file: "screenshot.ppm".to_string(),
            screenshot_sha256,
            checks,
            scenario_traces,
            trace_rows: report_trace_rows,
        };
        fs::write(locale_dir.join("report.json"), emit_report_json(&report))?;
    }

    if any_failure {
        return Err(format!(
            "ui verification failed for screen {} — see reports under {}",
            screen.screen_id,
            dir.display()
        )
        .into());
    }

    Ok(())
}

/// Renders `rgba` (tightly packed RGBA8) as a binary PPM (P6), dropping the alpha channel —
/// PPM is RGB-only and this is host-side evidence output, not a runtime path (ADR-005 boundary
/// unaffected).
fn encode_ppm(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut bytes = format!("P6\n{width} {height}\n255\n").into_bytes();
    bytes.reserve((width as usize) * (height as usize) * 3);
    for pixel in rgba.chunks_exact(4) {
        bytes.extend_from_slice(&pixel[0..3]);
    }
    bytes
}

/// Parses the deterministic `node_id<TAB>hex` baseline format (sorted by node id when
/// [`bootstrap_baselines`] writes one) — a hand-rolled format instead of adding a JSON dependency
/// to this crate for one small file. A missing file is not an error: it means "not bootstrapped
/// yet", handled by the caller.
fn load_baselines(path: &Path) -> Result<Vec<(String, String)>, BoxError> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    let mut baselines = Vec::with_capacity(contents.lines().count());
    for (index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let node_id = parts
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{}: line {} is missing a node id", path.display(), index + 1))?;
        let hex = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{}: line {} is missing a hash", path.display(), index + 1))?;
        baselines.push((node_id.to_string(), hex.to_string()));
    }
    Ok(baselines)
}

/// Writes a fresh baseline file from every `ColorHash` check's measured hash, sorted by node id
/// for a deterministic file. Returns `false` (writes nothing) when there is nothing to baseline —
/// a screen with no `ColorHash`-annotated golden references.
fn bootstrap_baselines(path: &Path, checks: &[CheckResult]) -> Result<bool, BoxError> {
    let mut rows: Vec<(String, String)> = checks
        .iter()
        .filter_map(|check| match &check.payload {
            CheckPayload::ColorHash { measured_hex, .. } => {
                Some((check.node_id.clone(), measured_hex.clone()))
            }
            _ => None,
        })
        .collect();
    if rows.is_empty() {
        return Ok(false);
    }
    rows.sort();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut contents = String::new();
    for (node_id, hex) in rows {
        contents.push_str(&node_id);
        contents.push('\t');
        contents.push_str(&hex);
        contents.push('\n');
    }
    fs::write(path, contents)?;
    Ok(true)
}

/// Maps a Vulkan device name to the stable backend identity evidence is pinned against. Lavapipe
/// (Mesa's Vulkan software rasterizer, the CI reference backend) reports its device name as
/// `llvmpipe (LLVM ...)` after its rasterization core; every other name is lowercased with
/// non-alphanumeric runs collapsed to single dashes (e.g. `Apple M2` -> `apple-m2`).
fn normalize_backend_id(device_name: &str) -> String {
    if device_name.starts_with("llvmpipe") {
        return "lavapipe".to_string();
    }
    let mut normalized = String::with_capacity(device_name.len());
    let mut last_was_dash = true; // suppresses a leading dash
    for character in device_name.chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            normalized.push('-');
            last_was_dash = true;
        }
    }
    while normalized.ends_with('-') {
        normalized.pop();
    }
    normalized
}

fn wall_clock_iso(clock: WallClock) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        clock.year, clock.month, clock.day, clock.hours, clock.minutes, clock.seconds
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_minimal_ppm_dropping_alpha() {
        let rgba = [10, 20, 30, 255, 40, 50, 60, 128];
        let ppm = encode_ppm(2, 1, &rgba);
        assert_eq!(ppm, b"P6\n2 1\n255\n\x0a\x14\x1e\x28\x32\x3c".to_vec());
    }

    #[test]
    fn normalizes_lavapipe_and_other_device_names() {
        assert_eq!(normalize_backend_id("llvmpipe (LLVM 17.0.0, 256 bits)"), "lavapipe");
        assert_eq!(normalize_backend_id("Apple M2"), "apple-m2");
        assert_eq!(normalize_backend_id("NVIDIA GeForce RTX 4090"), "nvidia-geforce-rtx-4090");
    }

    #[test]
    fn formats_wall_clock_as_iso8601_utc() {
        assert_eq!(
            wall_clock_iso(WallClock { year: 2026, month: 1, day: 1, hours: 12, minutes: 0, seconds: 0 }),
            "2026-01-01T12:00:00Z"
        );
    }

    #[test]
    fn baseline_round_trip_bootstraps_then_loads() {
        let dir = std::env::temp_dir().join(format!(
            "mdux-verify-baseline-test-{}",
            std::process::id()
        ));
        let path = dir.join("en-US.txt");
        let checks = vec![CheckResult {
            check_id: "sedation-index::color_hash".to_string(),
            node_id: "sedation-index".to_string(),
            kind: mdux_ui_verify::CheckKind::ColorHash,
            requirement_id: None,
            outcome: CheckOutcome::NoBaseline,
            payload: CheckPayload::ColorHash {
                expected_hex: None,
                measured_hex: "abc123".to_string(),
            },
        }];

        assert!(bootstrap_baselines(&path, &checks).expect("bootstrap should succeed"));
        let loaded = load_baselines(&path).expect("baseline file should parse");
        assert_eq!(loaded, vec![("sedation-index".to_string(), "abc123".to_string())]);

        fs::remove_dir_all(&dir).expect("test temp dir should be removable");
    }

    #[test]
    fn missing_baseline_file_loads_as_empty_not_an_error() {
        let path = std::env::temp_dir().join("mdux-verify-baseline-does-not-exist.txt");
        assert_eq!(load_baselines(&path).expect("missing file is not an error"), Vec::new());
    }
}
