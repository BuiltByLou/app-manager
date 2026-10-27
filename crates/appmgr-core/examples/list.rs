//! Utilitário de inspeção: `cargo run --example list`
//! Mostra o inventário tal como a interface o vai receber.

use appmgr_core::inventory::{build_inventory, InstallKind, Origin};
use appmgr_core::real::RealProbe;

fn main() {
    let started = std::time::Instant::now();
    let apps = build_inventory(&RealProbe::new());
    let elapsed = started.elapsed();

    let mut apps = apps;
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    for app in &apps {
        let kind = match app.kind {
            InstallKind::User => "user",
            InstallKind::System if app.protected => "sys*",
            InstallKind::System => "sys ",
        };
        let origin = match app.origin {
            Origin::Apt => "apt",
            Origin::Flatpak => "flatpak",
            Origin::AppImage => "appimage",
        };
        println!(
            "{kind} {origin:<8} {:<40} {:<20} {}",
            app.name,
            app.version.as_deref().unwrap_or("-"),
            app.package.as_deref().unwrap_or("-")
        );
    }

    let user = apps
        .iter()
        .filter(|a| a.kind == InstallKind::User)
        .count();
    println!(
        "\n{} apps ({} do utilizador, {} do sistema) em {:?}",
        apps.len(),
        user,
        apps.len() - user,
        elapsed
    );
}
