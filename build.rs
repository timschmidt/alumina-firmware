fn main() -> anyhow::Result<()> {
    // Propagate ESP-IDF configuration and link arguments across Cargo's build-script boundary.
    embuild::build::CfgArgs::output_propagated("ESP_IDF")?;
    embuild::build::LinkArgs::output_propagated("ESP_IDF")
}
