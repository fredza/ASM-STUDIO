mod app;
mod assemble;
mod debugger;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_title("ASM Studio"),
        ..Default::default()
    };
    eframe::run_native(
        "ASM Studio",
        native_options,
        Box::new(|_cc| Ok(Box::new(app::App::new()))),
    )
}
