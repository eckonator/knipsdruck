// SPDX-License-Identifier: MIT OR Apache-2.0
//! Knipsdruck: Fotos von Dokumenten druckfertig machen.

// Unter Windows im Release kein zusätzliches Konsolenfenster öffnen
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod corners;
mod devtools;
mod dialogs;
mod document;
mod fonts;
mod i18n;
mod icons;
mod phone_ui;
mod recent;
mod self_update;
mod settings;
mod settings_ui;
mod theme;
mod update;
mod widgets;
mod worker;

use eframe::egui;

use crate::settings::Settings;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    self_update::clean_up();
    let dev = devtools::DevTools::from_env();
    let mut settings = Settings::load();
    if dev.active() {
        settings::set_read_only();
    }
    if let Some(l) = &dev.lang {
        settings.language = if l == "en" {
            settings::LangSetting::En
        } else {
            settings::LangSetting::De
        };
    }
    if let Some(t) = &dev.theme {
        settings.theme = if t == "dunkel" || t == "dark" {
            settings::ThemeSetting::Dark
        } else {
            settings::ThemeSetting::Light
        };
    }
    i18n::apply_setting(settings.language);
    let mut dev = Some(dev);
    let renderer = match std::env::var("KD_RENDERER").as_deref() {
        Ok("glow" | "opengl") => eframe::Renderer::Glow,
        Ok("wgpu") => eframe::Renderer::Wgpu,
        _ if settings.opengl_fallback => eframe::Renderer::Glow,
        _ => eframe::Renderer::Wgpu,
    };
    let created = std::rc::Rc::new(std::cell::Cell::new(false));
    let mut result = run(renderer, settings.clone(), &mut dev, &created);
    // Scheitert wgpu schon beim Einrichten (Treiber, virtuelle Maschinen, Remote Desktop): OpenGL versuchen
    if let Err(e) = &result
        && renderer == eframe::Renderer::Wgpu
        && !created.get()
    {
        log::error!("Grafik über wgpu nicht verfügbar ({e}), versuche OpenGL");
        settings.opengl_fallback = true;
        settings.save();
        result = run(eframe::Renderer::Glow, settings, &mut dev, &created);
    }
    if let Err(e) = &result {
        log::error!("Programm beendet mit Fehler: {e}");
    }
    result
}

fn run(
    renderer: eframe::Renderer,
    settings: Settings,
    dev: &mut Option<devtools::DevTools>,
    created: &std::rc::Rc<std::cell::Cell<bool>>,
) -> eframe::Result {
    log::info!("Grafik: {renderer}");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Knipsdruck")
            // Wayland sucht Icon und Namen über die .desktop-Datei mit diesem Namen (assets/knipsdruck.desktop)
            .with_app_id("knipsdruck")
            .with_inner_size(devtools::DevTools::window_size().unwrap_or([1280.0, 860.0]))
            .with_min_inner_size([760.0, 560.0])
            .with_drag_and_drop(true)
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon-256.png")).unwrap_or_default(),
            ),
        renderer,
        wgpu_options: wgpu_options(),
        ..Default::default()
    };
    let created = std::rc::Rc::clone(created);
    let dev = dev.take().unwrap_or_default();
    eframe::run_native(
        "Knipsdruck",
        options,
        Box::new(move |cc| {
            created.set(true);
            Ok(Box::new(app::App::new(cc, settings, dev)))
        }),
    )
}

/// wgpu-Einstellungen. Unter Windows nur DirectX 12 (Vulkan-Treiber stürzten bei manchen Intel-Grafikchips beim
/// Start ab, ohne dass sich das abfangen ließ). `WGPU_BACKEND` überschreibt das.
fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew, wgpu};
    let mut options = WgpuConfiguration::default();
    if cfg!(windows) {
        let mut setup = WgpuSetupCreateNew::without_display_handle();
        setup.instance_descriptor.backends = wgpu::Backends::from_env().unwrap_or(wgpu::Backends::DX12);
        options.wgpu_setup = WgpuSetup::CreateNew(setup);
    }
    options
}
