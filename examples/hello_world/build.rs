fn main() -> Result<(), Box<dyn std::error::Error>> {
    trustsc_build::MeduiScreen::new("hello_world.medui")
        .surface(800, 480)
        .compile()
}
