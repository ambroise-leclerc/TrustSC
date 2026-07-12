fn main() -> Result<(), Box<dyn std::error::Error>> {
    trustsc_build::MeduiScreen::new("neurosense.medui")
        .surface(1920, 1080)
        .compile()?;
    // Phase 1 (Hugging Face-style demonstrator) points this at generated/models/eeg-demo/package.json;
    // Phase 2 (production) repoints it at a manufacturer's own clinically-qualified weights baked
    // by the same tools/trustsc-ml-baker pipeline — zero change below this line (ADR-017 §2).
    trustsc_build::ModelPackage::new("../../generated/models/eeg-demo/package.json").compile()?;
    trustsc_build::ScenarioSet::new("verify/scenarios").compile()
}
