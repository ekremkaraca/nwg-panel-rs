use gdk4 as gdk;
use gtk4 as gtk;
use std::path::Path;

pub fn load_user_css_if_exists(display: &gdk::Display, path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    // Load Omarchy theme if it exists
    let omarchy_path = Path::new("/home/ekrem/.config/omarchy/current/theme/gtk.css");
    if omarchy_path.exists() {
        let provider = gtk::CssProvider::new();
        // Best-effort: if the theme fails to load, continue with user CSS.
        let _ = provider.load_from_path(omarchy_path);
        gtk::style_context_add_provider_for_display(
            display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    // Load user CSS
    let provider = gtk::CssProvider::new();
    // Note: gtk4-rs' CssProvider::load_from_path is best-effort (returns ()), and GTK
    // will print parser errors to stderr.
    provider.load_from_path(path);
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    Ok(())
}
