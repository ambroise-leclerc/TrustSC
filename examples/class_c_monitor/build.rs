fn main() -> Result<(), Box<dyn std::error::Error>> {
    mdux_build::MeduiScreen::new("neurosense.medui")
        .surface(1280, 720)
        .compile()
}
