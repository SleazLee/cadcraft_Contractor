//! Contractor controls are a command client; the engine owns every drawing edit.
pub mod gpu;
pub(crate) mod i18n;
use crate::CadApp;
use cadcraft_contractor::view3d::{Camera, Scene};
use cadcraft_contractor::{self as model, MemberRecord, Representation, Shape};
use cadcraft_doc::Handle;
use egui::{Color32, DragValue, Sense, Vec2};
use serde_json::{Value, json};

pub struct State {
    pub gpu: Option<gpu::Renderer>,
    size: String,
    role: String,
    flat: bool,
    bottom: f64,
    width: f64,
    depth: f64,
    height: f64,
    diameter: f64,
    pile: bool,
    alignment: String,
    replace: bool,
    spacing: f64,
    count: u32,
    export_path: String,
    export_mode: String,
    paper: String,
    scale: f64,
    camera: Camera,
    scene: Scene,
    uid: u64,
    revision: u64,
    tool_uid: u64,
    reference: bool,
    texture: Option<egui::TextureHandle>,
    cache: Option<(Camera, usize, usize, Vec<Handle>)>,
    edit: Option<Edit>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            gpu: None,
            size: "2x10".into(),
            role: "joist".into(),
            flat: false,
            bottom: 0.0,
            width: 1.5,
            depth: 9.25,
            height: 96.0,
            diameter: 10.0,
            pile: false,
            alignment: "center".into(),
            replace: false,
            spacing: 16.0,
            count: 2,
            export_path: "framing.dwg".into(),
            export_mode: "plan".into(),
            paper: "ANSI D".into(),
            scale: 1.0 / 48.0,
            camera: Camera::default(),
            scene: Scene::default(),
            uid: 0,
            revision: 0,
            tool_uid: 0,
            reference: true,
            texture: None,
            cache: None,
            edit: None,
        }
    }
}
struct Edit {
    uid: u64,
    handle: Handle,
    revision: u64,
    record: MemberRecord,
    bottom: f64,
    length: f64,
    width: f64,
    depth: f64,
    height: f64,
    diameter: f64,
    flat: bool,
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(DragValue::new(value).speed(0.25).max_decimals(4));
    });
}
fn choice(ui: &mut egui::Ui, id: &str, value: &mut String, choices: &[&str]) {
    egui::ComboBox::from_id_salt(id).selected_text(value.as_str()).width(150.0).show_ui(ui, |ui| {
        for c in choices {
            ui.selectable_value(value, (*c).into(), *c);
        }
    });
}
fn options(st: &State) -> Value {
    json!({"size":st.size,"flat":st.flat,"bottom":st.bottom,"width":st.width,"depth":st.depth,"role":st.role,"alignment":st.alignment,"height":st.height,"diameter":st.diameter,"shape":if st.pile {"pile"} else {"post"}})
}

pub fn panel(app: &mut CadApp, ui: &mut egui::Ui) {
    if let Ok(st) = app.session.state()
        && app.contractor.tool_uid != st.uid
    {
        let inch = 25.4 / model::unit_mm(&st.doc).unwrap_or(25.4);
        app.contractor.width = 1.5 * inch;
        app.contractor.depth = 9.25 * inch;
        app.contractor.height = 96.0 * inch;
        app.contractor.diameter = 10.0 * inch;
        app.contractor.spacing = 16.0 * inch;
        app.contractor.bottom = 0.0;
        app.contractor.tool_uid = st.uid;
    }
    egui::ScrollArea::vertical().id_salt("contractor-tools").show(ui,|ui| {
        ui.heading("Floor framing");
        let current=app.session.doc().map(model::representation).unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            for (label,mode,rep) in [("Plan","plan",Representation::Plan),("3D","model3d",Representation::Model3d),("Engineering","engineering",Representation::Engineering)] {
                if ui.selectable_label(current==rep,label).clicked() {let _=app.run("contractor.view",json!({"mode":mode}));}
            }
        });
        ui.separator(); ui.label("Lumber");
        choice(ui,"cc-size",&mut app.contractor.size,&["2x2","2x4","2x6","2x8","2x10","2x12","custom"]);
        choice(ui,"cc-role",&mut app.contractor.role,&["joist","beam","board","blocking","rim"]);
        ui.checkbox(&mut app.contractor.flat,"Lay flat");
        if app.contractor.size=="custom" {field(ui,"Width",&mut app.contractor.width);field(ui,"Depth",&mut app.contractor.depth);}
        field(ui,"Bottom elevation",&mut app.contractor.bottom);
        if ui.button("Use selected member’s top").clicked() {
            let hs=app.session.selection();
            if let (Some(h),Ok(d))=(hs.first(),app.session.doc()) && let Ok(m)=model::resolve(d,*h) {
                app.contractor.bottom=m.top();
            }
        }
        if let Ok(d)=app.session.doc() {
            match model::unit_mm(d) { Ok(mm)=>{
                let name=match d.header.i64("INSUNITS",0) {1=>"inches",2=>"feet",4=>"mm",5=>"cm",6=>"metres",_=>"units"};
                ui.small(format!("Values in drawing {name}"));
                if let Ok((w,h))=model::lumber(&app.contractor.size,mm,app.contractor.flat) {ui.small(format!("Actual section: {w:.3} × {h:.3}"));}
            },Err(e)=>{ui.colored_label(crate::theme::Tokens::get().warn,e.to_string());}}
        }
        if ui.button("Place board — pick two points").clicked() {
            let p=options(&app.contractor);
            if app.run("contractor.tool",p).is_ok() {let _=app.run("contractor.view",json!({"mode":"plan"}));app.start("contractor.board");}
        }
        ui.separator();ui.label("Posts and pilings");
        ui.checkbox(&mut app.contractor.pile,"Round piling");
        if app.contractor.pile {field(ui,"Diameter",&mut app.contractor.diameter);} else {field(ui,"Post width",&mut app.contractor.width);field(ui,"Post depth",&mut app.contractor.depth);}
        field(ui,"Height",&mut app.contractor.height);
        if ui.button("Place support — pick center").clicked() {
            let mut p=options(&app.contractor);p["role"]=json!(if app.contractor.pile {"piling"} else {"post"});
            if app.run("contractor.tool",p).is_ok() {let _=app.run("contractor.view",json!({"mode":"plan"}));app.start("contractor.post");}
        }
        ui.separator();ui.label("Existing drawing");
        choice(ui,"cc-alignment",&mut app.contractor.alignment,&["center","left","right"]);
        ui.checkbox(&mut app.contractor.replace,"Replace selected source objects");
        if ui.button("Convert selected lines / circles").clicked() {
            let mut p=options(&app.contractor);p["replace"]=json!(app.contractor.replace);
            let _=app.run("contractor.convert",p);
        }
        ui.separator();ui.label("Repeat selected member");
        field(ui,"On-center spacing",&mut app.contractor.spacing);
        ui.horizontal(|ui|{ui.label("Count, including original");ui.add(DragValue::new(&mut app.contractor.count).range(2..=10_000));});
        if ui.button("Repeat").clicked() {let _=app.run("contractor.repeat",json!({"spacing":app.contractor.spacing,"count":app.contractor.count}));}
        ui.separator();inspector(app,ui);
        ui.separator();ui.label("Blueprint export");
        ui.text_edit_singleline(&mut app.contractor.export_path).on_hover_text("File path ending in .pdf, .dxf, or .dwg");
        choice(ui,"cc-export-rep",&mut app.contractor.export_mode,&["plan","engineering","model3d"]);
        choice(ui,"cc-paper",&mut app.contractor.paper,&["ANSI D","ANSI C","ANSI B","Letter","A3","A4"]);
        egui::ComboBox::from_id_salt("cc-scale").selected_text(format!("Scale 1:{:.0}",1.0/app.contractor.scale)).show_ui(ui,|ui|{
            for (ratio,label) in [(12.0,"1 inch = 1 foot"),(24.0,"½ inch = 1 foot"),(48.0,"¼ inch = 1 foot"),(96.0,"⅛ inch = 1 foot"),(50.0,"1:50"),(100.0,"1:100")] {ui.selectable_value(&mut app.contractor.scale,1.0/ratio,label);}
        });
        let mut denominator=1.0/app.contractor.scale;
        ui.horizontal(|ui|{ui.label("Custom scale 1:");if ui.add(DragValue::new(&mut denominator).range(1.0..=10_000.0).speed(1.0)).changed(){app.contractor.scale=1.0/denominator;}});
        if ui.button("Export PDF / DXF / DWG").clicked() {
            let _=app.run("contractor.export",json!({"path":app.contractor.export_path,"representation":app.contractor.export_mode,"paper":app.contractor.paper,"scale":app.contractor.scale,"fit":false}));
        }
        ui.separator();
        if ui.button("Add sample floor framing").clicked() {let _=app.run("contractor.sample",json!({}));let _=app.session.zoom_extents();}
        ui.small("Drawing geometry; no structural load calculations.");
    });
}

fn inspector(app: &mut CadApp, ui: &mut egui::Ui) {
    ui.label("Selected member");
    let hs = app.session.selection();
    let Some(h) = hs.first().copied().filter(|_| hs.len() == 1) else {
        ui.small("Select one board, post, or piling to edit it.");
        app.contractor.edit = None;
        return;
    };
    let Ok(doc) = app.session.doc() else {
        return;
    };
    let m = match model::resolve(doc, h) {
        Ok(m) => m,
        Err(e) => {
            if doc.entity(h).is_some_and(|e| model::tagged(e)) {
                ui.colored_label(crate::theme::Tokens::get().warn, e.to_string());
            }
            return;
        }
    };
    let (uid, revision) = app.session.state().map(|s| (s.uid, s.revision)).unwrap_or((0, 0));
    ui.small(format!("Bottom {:.3}  Top {:.3}", m.bottom(), m.top()));
    if app.contractor.edit.as_ref().is_none_or(|e| e.uid != uid || e.handle != h || e.revision != revision) {
        let (length, width, depth, height, diameter) = match m.record.shape {
            Shape::Board { length, width, depth } => {
                (length * m.insert.scale.x.abs(), width * m.insert.scale.y.abs(), depth * m.insert.scale.z.abs(), 0.0, 0.0)
            }
            Shape::Post { width, depth, height } => {
                (0.0, width * m.insert.scale.x.abs(), depth * m.insert.scale.y.abs(), height * m.insert.scale.z.abs(), 0.0)
            }
            Shape::Pile { diameter, height } => (0.0, 0.0, 0.0, height * m.insert.scale.z.abs(), diameter * m.insert.scale.x.abs()),
        };
        app.contractor.edit = Some(Edit {
            uid,
            handle: h,
            revision,
            bottom: m.bottom(),
            flat: width > depth,
            length,
            width,
            depth,
            height,
            diameter,
            record: m.record,
        });
    }
    let Some(e) = app.contractor.edit.as_mut() else {
        return;
    };
    ui.text_edit_singleline(&mut e.record.label).on_hover_text("Member label");
    ui.text_edit_singleline(&mut e.record.role).on_hover_text("Member role");
    match e.record.shape {
        Shape::Board { .. } => {
            choice(ui, "cc-edit-size", &mut e.record.profile, &["2x2", "2x4", "2x6", "2x8", "2x10", "2x12", "custom"]);
            if e.record.profile != "custom" {
                ui.checkbox(&mut e.flat, "Flat orientation");
            }
            field(ui, "Length", &mut e.length);
            if e.record.profile == "custom" {
                field(ui, "Width", &mut e.width);
                field(ui, "Depth", &mut e.depth);
                if ui.button("Swap width / depth").clicked() {
                    std::mem::swap(&mut e.width, &mut e.depth);
                }
            }
        }
        Shape::Post { .. } => {
            field(ui, "Width", &mut e.width);
            field(ui, "Depth", &mut e.depth);
            field(ui, "Height", &mut e.height);
        }
        Shape::Pile { .. } => {
            field(ui, "Diameter", &mut e.diameter);
            field(ui, "Height", &mut e.height);
        }
    }
    field(ui, "Bottom", &mut e.bottom);
    if ui.button("Apply member changes").clicked() {
        let mut p = json!({"handle":e.handle.hex(),"bottom":e.bottom,"role":e.record.role,"label":e.record.label});
        match e.record.shape {
            Shape::Board { .. } => {
                p["size"] = json!(e.record.profile);
                if e.record.profile != "custom" {
                    p["flat"] = json!(e.flat);
                }
                p["length"] = json!(e.length);
                if e.record.profile == "custom" {
                    p["width"] = json!(e.width);
                    p["depth"] = json!(e.depth);
                }
            }
            Shape::Post { .. } => {
                p["width"] = json!(e.width);
                p["depth"] = json!(e.depth);
                p["height"] = json!(e.height);
            }
            Shape::Pile { .. } => {
                p["diameter"] = json!(e.diameter);
                p["height"] = json!(e.height);
            }
        }
        let _ = app.run("contractor.set", p);
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
        let _ = app.run("select", json!({"handles":h.map(|h|vec![h.hex()]).unwrap_or_default()}));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn inspect(app: &mut CadApp) {
        let context = egui::Context::default();
        let mut frame = context.run_ui(egui::RawInput::default(), |ui| inspector(app, ui));
        frame.textures_delta.clear();
    }

    #[test]
    fn property_draft_tracks_the_drawing_even_when_handles_and_revisions_match() {
        let mut session = cadcraft_engine::Session::new();
        session.execute("contractor.board", &json!({"start":[0,0],"end":[120,0],"size":"2x10"})).unwrap();
        let mut app = CadApp::new(session, crate::Services::default());
        inspect(&mut app);
        let old = app.contractor.edit.as_ref().unwrap();
        let (uid, handle, revision) = (old.uid, old.handle, old.revision);
        assert_eq!(old.record.profile, "2x10");
        app.session.new_drawing(false);
        app.session.execute("contractor.board", &json!({"start":[0,0],"end":[120,0],"size":"2x12"})).unwrap();
        inspect(&mut app);
        let new = app.contractor.edit.as_ref().unwrap();
        assert_eq!((new.handle, new.revision), (handle, revision));
        assert_ne!(new.uid, uid);
        assert_eq!(new.record.profile, "2x12");
        assert_eq!(new.depth, 11.25);
    }
}
