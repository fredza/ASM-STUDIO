mod abi;
mod app;
mod assemble;
mod binfmt;
mod breakpoint;
mod debugger;
mod desdec;
mod diagnostic;
mod disasm;
mod effects;
mod encoding;
mod exercise;
mod explain;
mod i18n;
mod license;
mod pe_link;
mod project;
mod simd;
mod srcmap;
mod syntax;
mod syscall;
mod theme;
mod trial;
mod tutorial;
mod updater;
mod version;
mod winerun;

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
        // Le premier argument, quand il y en a un, est le fichier à ouvrir :
        // c'est ce qu'attend un gestionnaire de fichiers (`%f` dans le
        // `.desktop`) comme un outil qui passe la main.
        Box::new(|_cc| {
            let opening = std::env::args_os().nth(1).map(std::path::PathBuf::from);
            Ok(Box::new(app::App::new_opening(opening)))
        }),
    )
}
