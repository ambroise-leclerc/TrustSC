mdux::include_medui_screen!();

use std::{cell::Cell, rc::Rc};

use mdux::{
    ComplianceProgram, DeviceContext, FrameworkBuilder, Hazard, Requirement, RequirementId,
    SafetyClass, TextInputModel, UiSdkConfig, VerificationCase, VerificationMethod, WidgetEvent,
};

/// Synthetic EEG: two drifting spectral peaks over pseudo-noise; the sedation index follows the
/// dominant peak. Stands in for the acquisition front-end a real device would have.
struct EegSimulator {
    tick: u32,
    noise: u32,
}

impl EegSimulator {
    fn tick(&mut self) -> (i64, [f32; 64]) {
        self.tick += 1;
        let time = self.tick as f32 / 60.0;
        let peak_a = 12.0 + 6.0 * (time / 5.0).sin();
        let peak_b = 38.0 + 10.0 * (time / 9.0).cos();
        let mut row = [0.0f32; 64];
        for (bin, sample) in row.iter_mut().enumerate() {
            self.noise = self.noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (self.noise >> 24) as f32 / 255.0 * 0.12;
            let lobe = |peak: f32, width: f32| (-((bin as f32 - peak) / width).powi(2)).exp();
            *sample = (0.85 * lobe(peak_a, 4.0) + 0.55 * lobe(peak_b, 7.0) + noise).min(1.0);
        }
        let index = (46.0 + 18.0 * (time / 7.0).sin()).clamp(0.0, 99.0) as i64;
        (index, row)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = DeviceContext::new(
        "Acme Medical",
        "NeuroSense 500",
        "neurosense-ui",
        "0.1.0",
        SafetyClass::C,
    )?;

    let mut compliance = ComplianceProgram::new(device.clone());
    let req_index = RequirementId::new("REQ-NS-001")?;
    let req_stream = RequirementId::new("REQ-NS-002")?;
    let req_status = RequirementId::new("REQ-NS-003")?;
    let req_ack = RequirementId::new("REQ-NS-004")?;
    let req_patient_id = RequirementId::new("REQ-NS-005")?;
    for (id, verification_id, title) in [
        (&req_index, "VER-NS-001", "Display the sedation index, refreshed every frame"),
        (&req_stream, "VER-NS-002", "Display the spectral stream with visible freshness"),
        (&req_status, "VER-NS-003", "Keep the system status permanently visible"),
        (&req_ack, "VER-NS-004", "Capture operator acknowledgment of the active alert"),
        (
            &req_patient_id,
            "VER-NS-005",
            "Bound patient identifier entry to the approved character set and length",
        ),
    ] {
        compliance.add_requirement(Requirement::new(
            id.clone(),
            title,
            "IEC62304-5.2",
            "Verified by windowed demonstration and headless smoke",
        )?);
        compliance.add_verification(VerificationCase::new(
            verification_id,
            id.clone(),
            VerificationMethod::Demonstration,
            "Windowed run on the development host",
        )?);
    }
    compliance.add_hazard(Hazard::new(
        "HAZ-NS-001",
        "A stale or frozen sedation index misleads the anesthesiologist",
        vec![req_index, req_stream],
    )?);

    let screen = medui_screen::screen();
    let framework = FrameworkBuilder::new()
        .with_device(device)
        .with_compliance(compliance)
        .with_ui(UiSdkConfig::vulkansc_class_c(
            medui_screen::GENERATED_MEDUI_SURFACE.0,
            medui_screen::GENERATED_MEDUI_SURFACE.1,
            12,
            32 * 1024 * 1024,
            256,
        ))
        .with_screen(screen)
        .build()?;

    let mut simulator = EegSimulator { tick: 0, noise: 0x9E37_79B9 };
    // The simulator raises an alert periodically; the operator acknowledges it with the ACK
    // button (REQ-NS-004). Shared between the input and realtime closures.
    let alert_active = Rc::new(Cell::new(false));
    let alert_for_input = Rc::clone(&alert_active);
    // The application owns the patient-identifier buffer (ADR-015 controlled component) and
    // echoes it into the frame; charset and length stay bounded by the compiled screen
    // (REQ-NS-005).
    let mut patient_id = TextInputModel::new("PATIENT_ID", 16);

    mdux_vulkan_winit::App::new(framework, screen)
        .with_input(move |events, frame| {
            for event in events.drain() {
                match event {
                    WidgetEvent::ButtonPressed { source: "ACK_BUTTON" } => {
                        alert_for_input.set(false);
                    }
                    other => {
                        patient_id.apply(&other);
                    }
                }
            }
            frame
                .set_text("PATIENT_ID", patient_id.as_str())
                .expect("PATIENT_ID wiring");
        })
        .with_realtime(move |frame| {
            let (index, row) = simulator.tick();
            // A synthetic alert fires every ~20 s at the nominal 60 Hz and latches until the
            // operator acknowledges it.
            if simulator.tick % 1200 == 0 {
                alert_active.set(true);
            }
            let status = if alert_active.get() { 1 } else { 0 };
            frame.set_number("SEDATION_INDEX", index).expect("SEDATION_INDEX wiring");
            frame.set_status("MONITOR_STATUS", status).expect("MONITOR_STATUS wiring");
            frame.push_row("EEG_DSA", &row).expect("EEG_DSA wiring");
        })
        .run_from_env()
}
