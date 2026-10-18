//! Contractor controls are a command client; the engine owns every drawing edit.
pub mod gpu;
pub(crate) mod i18n;
use crate::CadApp;
use cadcraft_contractor::view3d::{Camera, Scene};
use cadcraft_contractor::{self as model, Representation};
use cadcraft_doc::Handle;
use egui::{Color32, Sense, Vec2};
use serde_json::{Value, json};

mod properties;
pub use properties::{member_type, panel, properties};

pub struct State {
    pub gpu: Option<gpu::Renderer>,
    forms: properties::Forms,
    camera: Camera,
    scene: Scene,
    uid: u64,
    revision: u64,
    reference: bool,
    texture: Option<egui::TextureHandle>,
    cache: Option<(Camera, usize, usize, Vec<Handle>)>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            gpu: None,
            forms: properties::Forms::default(),
            camera: Camera::default(),
            scene: Scene::default(),
            uid: 0,
            revision: 0,
            reference: true,
            texture: None,
            cache: None,
        }
    }
}

pub fn canvas(app: &mut CadApp, ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    app.canvas.rect = Some(rect);
    let response = ui.allocate_rect(rect, Sense::click_and_drag());
    let Ok(st) = app.session.state() else {
        return;
    };
    let doc = st.doc.clone();
    let (uid, revision) = (st.uid, st.revision);
    if app.contractor.uid != uid || app.contractor.revision != revision {
        app.contractor.scene = match Scene::build(&doc) {
            Ok(scene) => scene,
            Err(e) => {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    e.to_string(),
                    egui::FontId::proportional(14.0),
                    crate::theme::Tokens::get().warn,
                );
                return;
            }
        };
        if app.contractor.uid != uid || app.contractor.scene.triangles.is_empty() {
            app.contractor.camera.fit(&app.contractor.scene, f64::from(rect.aspect_ratio()));
        }
        app.contractor.uid = uid;
        app.contractor.revision = revision;
        app.contractor.cache = None;
    }
    let delta = ui.input(|i| i.pointer.delta());
    if response.dragged() {
        if ui.input(|i| i.modifiers.shift || i.pointer.secondary_down() || i.pointer.middle_down()) {
            let (r, u, _) = app.contractor.camera.basis();
            let scale = app.contractor.camera.height / f64::from(rect.height().max(1.0));
            app.contractor.camera.center = app.contractor.camera.center - r * f64::from(delta.x) * scale + u * f64::from(delta.y) * scale;
        } else {
            app.contractor.camera.yaw -= f64::from(delta.x) * 0.008;
            app.contractor.camera.pitch = (app.contractor.camera.pitch + f64::from(delta.y) * 0.008).clamp(-1.55, 1.55);
        }
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        app.contractor.camera.height = (app.contractor.camera.height * (-f64::from(scroll) * 0.003).exp()).clamp(1e-5, 1e12);
    }
    if response.double_clicked() {
        app.contractor.camera.fit(&app.contractor.scene, f64::from(rect.aspect_ratio()));
    }
    if app.contractor.camera.validate().is_err() {
        app.contractor.camera = Camera::default();
        app.contractor.camera.fit(&app.contractor.scene, f64::from(rect.aspect_ratio()));
    }
    let density = f64::from(ui.ctx().pixels_per_point().clamp(0.1, 8.0));
    let wanted_width = f64::from(rect.width().max(1.0)) * density;
    let wanted_height = f64::from(rect.height().max(1.0)) * density;
    let resolution_scale = (1600.0 / wanted_width).min(1200.0 / wanted_height).min(1.0);
    let width = (wanted_width * resolution_scale).round().max(1.0) as usize;
    let height = (wanted_height * resolution_scale).round().max(1.0) as usize;
    if response.clicked()
        && let Some(p) = response.interact_pointer_pos()
    {
        let x = f64::from((p.x - rect.left()) / rect.width()) * width as f64;
        let y = f64::from((p.y - rect.top()) / rect.height()) * height as f64;
        let h = app.contractor.scene.pick(&app.contractor.camera, width, height, x, y);
        let handles = h.into_iter().collect::<Vec<_>>();
        if app.session.current_prompt().is_some_and(|p| p.accept.select) {
            if let Err(e) = app.session.input(cadcraft_engine::Input::Pick(handles)) {
                app.session.echo(e.to_string());
            }
        } else {
            let _ = app.run("select", json!({"handles":handles.iter().map(|h|h.hex()).collect::<Vec<_>>()}));
        }
    }
    ui.painter().rect_filled(rect, 0.0, crate::theme::Tokens::get().canvas);
    if app.contractor.reference {
        let mut reference = (*doc).clone();
        let hs = reference.model.iter().filter(|e| model::resolve(&doc, e.handle).is_ok()).map(|e| e.handle).collect::<Vec<_>>();
        for h in hs {
            reference.remove_entity(h);
        }
        let list = cadcraft_render::build(&reference, &cadcraft_doc::Space::Model, &Default::default());
        for prim in &list.prims {
            if prim.kind != cadcraft_render::Kind::Polyline {
                continue;
            }
            for pair in list.points(prim).windows(2) {
                let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else {
                    continue;
                };
                let to_screen = |p: cadcraft_geom::Vec2| {
                    let q = app.contractor.camera.project(p.to3(0.0), width, height);
                    if !q.is_finite() || q.x.abs() > 1e7 || q.y.abs() > 1e7 {
                        return None;
                    }
                    Some(egui::pos2(rect.left() + q.x as f32 / width as f32 * rect.width(), rect.top() + q.y as f32 / height as f32 * rect.height()))
                };
                if let (Some(a), Some(b)) = (to_screen(*a), to_screen(*b)) {
                    ui.painter().line_segment([a, b], egui::Stroke::new(0.7, crate::theme::Tokens::get().text_dim));
                }
            }
        }
    }
    let selected = app.session.selection();
    let key = (app.contractor.camera, width, height, selected.clone());
    if let Some(gpu) = &mut app.contractor.gpu {
        match gpu.render(&app.contractor.scene, &app.contractor.camera, width as u32, height as u32, &selected) {
            Ok(texture) => {
                ui.painter().image(texture, rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
            }
            Err(e) => {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    e.to_string(),
                    egui::FontId::proportional(14.0),
                    crate::theme::Tokens::get().warn,
                );
            }
        }
    } else {
        if app.contractor.cache.as_ref() != Some(&key) {
            match model::view3d::raster(&app.contractor.scene, &app.contractor.camera, width, height, &selected) {
                Ok(frame) => {
                    let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &frame.rgba);
                    if let Some(texture) = &mut app.contractor.texture {
                        texture.set(image, egui::TextureOptions::LINEAR);
                    } else {
                        app.contractor.texture = Some(ui.ctx().load_texture("contractor-3d", image, egui::TextureOptions::LINEAR));
                    }
                    app.contractor.cache = Some(key);
                }
                Err(e) => {
                    app.contractor.texture = None;
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        e.to_string(),
                        egui::FontId::proportional(14.0),
                        crate::theme::Tokens::get().warn,
                    );
                }
            }
        }
        if let Some(texture) = &app.contractor.texture {
            ui.painter().image(texture.id(), rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
    }
    let controls = egui::Rect::from_min_size(rect.min + Vec2::splat(8.0), Vec2::new(470.0, 34.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(controls), |ui| {
        ui.horizontal(|ui| {
            if ui.button("Fit model").clicked() {
                app.contractor.camera.fit(&app.contractor.scene, f64::from(rect.aspect_ratio()));
            }
            ui.checkbox(&mut app.contractor.reference, "2D reference");
            ui.label("Drag: orbit · Shift/right drag: pan · Wheel: zoom");
        });
    });
}

/// Presentation-only annotations; source handles and the editable drawing remain unchanged.
pub fn presentation(d: &cadcraft_doc::Drawing) -> std::borrow::Cow<'_, cadcraft_doc::Drawing> {
    if model::representation(d) == Representation::Engineering
        && let Ok(mm) = model::unit_mm(d)
        && let Ok(out) = model::export_drawing(d, Representation::Engineering, 2.5 / mm * 48.0)
    {
        std::borrow::Cow::Owned(out)
    } else {
        std::borrow::Cow::Borrowed(d)
    }
}

pub fn render_png(app: &CadApp, p: &Value, width: u32, height: u32) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    let scene = Scene::build(app.session.doc().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut camera =
        if let Some(v) = p.get("camera") { serde_json::from_value::<Camera>(v.clone()).map_err(|e| e.to_string())? } else { app.contractor.camera };
    if p.get("fit").and_then(Value::as_bool).unwrap_or(true) {
        camera.fit(&scene, f64::from(width) / f64::from(height.max(1)));
    }
    let mut frame = model::view3d::raster(&scene, &camera, width as usize, height as usize, &app.session.selection()).map_err(|e| e.to_string())?;
    for pixel in frame.rgba.as_chunks_mut::<4>().0 {
        if pixel.get(3) == Some(&0) {
            pixel.copy_from_slice(&[29, 35, 42, 255]);
        }
    }
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(&frame.rgba, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}
