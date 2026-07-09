fn main() -> Result<(), Box<dyn std::error::Error>> {
    mdux_build::MeduiScreen::new("neurosense.medui")
        .surface(1920, 1080)
        .compile()?;
    mdux_build::ScenarioSet::new("verify/scenarios").compile()
}
