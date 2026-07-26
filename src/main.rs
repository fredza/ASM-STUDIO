mod app;
mod assemble;
mod debugger;
mod disasm;
mod explain;
mod i18n;
mod srcmap;
mod syntax;
mod syscall;

fn main() -> eframe::Result {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1100.0, 700.0])
        .with_title("ASM Studio");
    // Icône d'application (planche fournie, découpée dans assets/icon.png).
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "ASM Studio",
        native_options,
        Box::new(|_cc| Ok(Box::new(app::App::new()))),
    )
}
