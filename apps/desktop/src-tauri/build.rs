fn main() {
    // `pill_label` genera il permesso `allow-pill-label` della capability.
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(&["pill_label"]));
    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("generazione del contesto Tauri: {error:#}");
    }
}
