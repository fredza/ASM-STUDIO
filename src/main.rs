mod abi;
mod app;
mod assemble;
mod debugger;
mod diagnostic;
mod disasm;
mod effects;
mod encoding;
mod exercise;
mod explain;
mod i18n;
mod license;
mod srcmap;
mod syntax;
mod syscall;
mod tutorial;
mod updater;

fn main() -> eframe::Result {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1100.0, 700.0])
        .with_title("ASM Studio")
        // app_id utilisé par Wayland/GNOME pour associer la fenêtre au .desktop :
        // ~/.local/share/applications/asm-studio.desktop  +  icône asm-studio.png
        .with_app_id("asm-studio");
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
