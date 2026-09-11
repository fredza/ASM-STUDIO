use asm_studio::app;

fn main() -> eframe::Result {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1100.0, 700.0])
        .with_title("ASM Studio")
        // app_id utilisé par Wayland/GNOME pour associer la fenêtre au .desktop :
        // ~/.local/share/applications/asm-studio.desktop  +  icône asm-studio.png
        .with_app_id("asm-studio")
        // Décorations natives coupées : sans ça, réduire/agrandir/fermer sont
        // dessinés par le gestionnaire de fenêtres, à l'endroit — gauche ou
        // droite — que dicte SON thème, hors du contrôle de l'appli. La barre
        // de menu (`ui_chrome::menu_bar`) dessine désormais sa propre poignée
        // de déplacement et ses propres boutons, toujours à droite ; les bords
        // de la fenêtre gagnent leurs propres poignées de redimensionnement
        // (`ui_chrome::window_resize_handles`), pour que couper les
        // décorations ne rende pas la fenêtre plus figée qu'avant.
        .with_decorations(false);
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
