//! Item-specific placement drafts and selected-member Properties. Edits go through commands.
use crate::CadApp;
use cadcraft_contractor::{self as model, Member, Shape};
use cadcraft_doc::Handle;
use egui::DragValue;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct Draft {
    kind: String,
    values: BTreeMap<String, String>,
    dirty: BTreeSet<String>,
    mixed: BTreeSet<String>,
}
impl Draft {
    fn new(kind: &str, inch: f64) -> Self {
        let mut values = BTreeMap::new();
        for (key, n) in [
            ("bottom", 0.0),
            ("top", 0.0),
            ("width", if kind == "lumber" { 1.5 } else { 5.5 }),
            ("depth", if kind == "lumber" { 9.25 } else { 5.5 }),
            ("height", 96.0),
            ("depthBelowTop", 96.0),
            ("diameter", 10.0),
        ] {
            values.insert(key.into(), format!("{}", n * inch));
        }
        values.insert("size".into(), "2x10".into());
        values.insert("flat".into(), "false".into());
        values.insert("label".into(), String::new());
        values.insert(
            "role".into(),
            if kind == "lumber" {
                "joist"
            } else if kind == "pile" {
                "piling"
            } else {
                "post"
            }
            .into(),
        );
        Self { kind: kind.into(), values, dirty: BTreeSet::new(), mixed: BTreeSet::new() }
    }
    fn params(&self, changed: bool) -> Value {
        let mut p = json!({});
        for (key, value) in &self.values {
            if changed && !self.dirty.contains(key) {
                continue;
            }
            if key == "flat" {
                if let Ok(b) = value.parse::<bool>() {
                    p[key] = json!(b);
                }
            } else {
                p[key] = json!(value);
            }
        }
        if !changed {
            // Each placement type has independent defaults, and sends only its own fields.
            let keys = match self.kind.as_str() {
                "lumber" => vec!["size", "flat", "width", "depth", "bottom", "role", "label"],
                "post" => vec!["width", "depth", "height", "bottom", "role", "label"],
                _ => vec!["diameter", "top", "depthBelowTop", "role", "label"],
            };
            if let Some(o) = p.as_object_mut() {
                o.retain(|k, _| keys.contains(&k.as_str()));
            }
            p["kind"] = json!(self.kind);
            p["shape"] = json!(if self.kind == "pile" { "pile" } else { "post" });
        }
        p
    }
}
struct Edit {
    uid: u64,
    revision: u64,
    handles: Vec<Handle>,
    draft: Draft,
    cut: Option<f64>,
}
pub(super) struct Forms {
    uid: u64,
    kind: String,
    drafts: BTreeMap<String, Draft>,
    edit: Option<Edit>,
    pending_base: bool,
    spacing: String,
    count: u32,
    alignment: String,
    replace: bool,
    path: String,
    representation: String,
    paper: String,
    denominator: f64,
    report: Option<(u64, u64, Value)>,
    error: Option<String>,
}
impl Default for Forms {
    fn default() -> Self {
        Self {
            uid: 0,
            kind: "lumber".into(),
            drafts: BTreeMap::new(),
            edit: None,
            pending_base: false,
            spacing: "16".into(),
            count: 2,
            alignment: "center".into(),
            replace: false,
            path: "framing.dwg".into(),
            representation: "plan".into(),
            paper: "ANSI D".into(),
            denominator: 48.0,
            report: None,
            error: None,
        }
    }
}
fn sync(app: &mut CadApp) {
    let Ok(st) = app.session.state() else { return };
    if app.contractor.forms.uid != st.uid {
        let inch = 25.4 / model::unit_mm(&st.doc).unwrap_or(25.4);
        let f = &mut app.contractor.forms;
        f.uid = st.uid;
        f.edit = None;
        f.report = None;
        f.error = None;
        f.pending_base = false;
        f.spacing = format!("{}", 16.0 * inch);
        f.drafts = ["lumber", "post", "pile"].into_iter().map(|k| (k.into(), Draft::new(k, inch))).collect();
    }
    if app.contractor.forms.pending_base && !app.session.prompting() {
        let f = &mut app.contractor.forms;
        f.pending_base = false;
        if let Some(d) = f.drafts.get_mut(&f.kind) {
            for key in ["bottom", "top"] {
                if let Some(v) = st.contractor_tool.get(key).and_then(Value::as_f64) {
                    d.values.insert(key.into(), v.to_string());
                }
            }
        }
    }
}
fn field(ui: &mut egui::Ui, d: &mut Draft, key: &str, label: &str) {
    ui.horizontal(|ui| {
        let label = ui.label(label);
        let hint = if d.mixed.contains(key) { "Varies" } else { "" };
        let v = d.values.entry(key.into()).or_default();
        if ui.add(egui::TextEdit::singleline(v).desired_width(105.0).hint_text(hint)).labelled_by(label.id).changed() {
            d.dirty.insert(key.into());
        }
    });
}
fn choice(ui: &mut egui::Ui, id: &str, value: &mut String, items: &[(&str, &str)]) -> bool {
    let old = value.clone();
    let label = items.iter().find(|(v, _)| *v == value).map(|(_, label)| *label).unwrap_or("Varies");
    egui::ComboBox::from_id_salt(id).selected_text(label).width(145.0).show_ui(ui, |ui| {
        for (v, label) in items {
            ui.selectable_value(value, (*v).into(), *label);
        }
    });
    old != *value
}
fn editor(ui: &mut egui::Ui, d: &mut Draft, selected: bool) {
    if selected && !["lumber", "mixed"].contains(&d.kind.as_str()) {
        ui.label("Support type");
        if choice(ui, "cc-support-type", &mut d.kind, &[("post", "Rectangular post"), ("pile", "Round piling")]) {
            d.values.insert("shape".into(), d.kind.clone());
            d.dirty.insert("shape".into());
        }
    }
    field(ui, d, "label", "Label");
    field(ui, d, "role", "Role");
    match d.kind.as_str() {
        "lumber" => {
            ui.label("Wood size");
            let v = d.values.entry("size".into()).or_default();
            if choice(
                ui,
                "cc-wood-size",
                v,
                &[("2x2", "2×2"), ("2x4", "2×4"), ("2x6", "2×6"), ("2x8", "2×8"), ("2x10", "2×10"), ("2x12", "2×12"), ("custom", "Custom")],
            ) {
                d.dirty.insert("size".into());
            }
            let v = d.values.entry("flat".into()).or_default();
            if choice(ui, "cc-orientation", v, &[("false", "On edge"), ("true", "Flat")]) {
                d.dirty.insert("flat".into());
            }
            if d.values.get("size").is_some_and(|v| v == "custom") {
                field(ui, d, "width", "Actual width");
                field(ui, d, "depth", "Actual depth");
            }
            field(ui, d, "bottom", "Bottom elevation");
            if selected {
                field(ui, d, "length", "Engineering span");
                ui.separator();
                ui.label("Physical end setbacks");
                field(ui, d, "startSetback", "Start");
                field(ui, d, "endSetback", "End");
            }
        }
        "pile" => {
            field(ui, d, "diameter", "Diameter");
            field(ui, d, "top", "Top elevation");
            field(ui, d, "depthBelowTop", "Depth below top");
        }
        "post" => {
            field(ui, d, "width", "Width");
            field(ui, d, "depth", "Depth");
            field(ui, d, "height", "Height");
            field(ui, d, "bottom", "Bottom elevation");
            if selected {
                field(ui, d, "top", "Top elevation");
            }
        }
        _ => {
            field(ui, d, "bottom", "Bottom elevation");
        }
    }
    ui.small("Lengths: 9.25\", 1.2', 250 mm, or drawing units");
}
fn title(kind: &str) -> &'static str {
    match kind {
        "lumber" => "Lumber",
        "post" => "Post",
        "pile" => "Piling",
        _ => "Framing members",
    }
}
fn run(app: &mut CadApp, command: &str, params: Value) -> Option<Value> {
    match app.run(command, params) {
        Ok(v) => {
            app.contractor.forms.error = None;
            Some(v)
        }
        Err(e) => {
            app.contractor.forms.error = Some(e);
            None
        }
    }
}
fn place(app: &mut CadApp) {
    let f = &app.contractor.forms;
    let Some(d) = f.drafts.get(&f.kind) else { return };
    let (p, lumber) = (d.params(false), d.kind == "lumber");
    if run(app, "contractor.tool", p).is_some() {
        app.start(if lumber { "contractor.board" } else { "contractor.post" });
    }
}
fn reference_choices(app: &mut CadApp, ui: &mut egui::Ui) {
    if let Some(p) = app.session.current_prompt()
        && !p.keywords.is_empty()
        && !p.accept.select
    {
        ui.small(&p.message);
        ui.horizontal_wrapped(|ui| {
            for word in p.keywords {
                if ui.button(&word).clicked() {
                    app.cmdline(&word);
                }
            }
        });
    }
}
pub fn panel(app: &mut CadApp, ui: &mut egui::Ui) {
    sync(app);
    egui::ScrollArea::vertical().id_salt("contractor-tools").show(ui, |ui| {
        ui.heading("Build");
        let rep = app.session.doc().map(model::representation).unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            for (label, mode, r) in [
                ("Plan", "plan", model::Representation::Plan),
                ("3D", "model3d", model::Representation::Model3d),
                ("Engineering", "engineering", model::Representation::Engineering),
            ] {
                if ui.selectable_label(rep == r, label).clicked() {
                    run(app, "contractor.view", json!({"mode":mode}));
                }
            }
        });
        ui.separator();
        ui.label("New item");
        for kind in ["lumber", "post", "pile"] {
            if ui.selectable_label(app.contractor.forms.kind == kind, format!("New {}", title(kind).to_lowercase())).clicked() {
                app.contractor.forms.kind = kind.into();
                run(app, "select", json!({"handles":[]}));
                let _ = app.run("ui.panel.properties", Value::Null);
            }
        }
        ui.small("Configure this item in Properties.");
        if ui.button(format!("Place {}", title(&app.contractor.forms.kind).to_lowercase())).clicked() {
            place(app);
        }
        reference_choices(app, ui);
        ui.separator();
        ui.collapsing("Repeat", |ui| {
            ui.horizontal(|ui| {
                ui.label("On-center spacing");
                ui.add(egui::TextEdit::singleline(&mut app.contractor.forms.spacing).desired_width(80.0));
            });
            ui.horizontal(|ui| {
                ui.label("Count including original");
                ui.add(DragValue::new(&mut app.contractor.forms.count).range(2..=10000));
            });
            if ui.button("Repeat selected member").clicked() {
                run(app, "contractor.repeat", json!({"spacing":app.contractor.forms.spacing,"count":app.contractor.forms.count}));
            }
        });
        ui.collapsing("Convert selected lines / circles", |ui| {
            choice(ui, "cc-align", &mut app.contractor.forms.alignment, &[("center", "Centerline"), ("left", "Left edge"), ("right", "Right edge")]);
            ui.checkbox(&mut app.contractor.forms.replace, "Replace source objects");
            ui.small("Uses the current new-item properties.");
            if ui.button("Convert").clicked() {
                let f = &app.contractor.forms;
                if let Some(d) = f.drafts.get(&f.kind) {
                    let mut p = d.params(false);
                    p["replace"] = json!(f.replace);
                    p["alignment"] = json!(f.alignment);
                    run(app, "contractor.convert", p);
                }
            }
        });
        ui.collapsing("Blueprint export", |ui| {
            let f = &mut app.contractor.forms;
            ui.text_edit_singleline(&mut f.path);
            choice(ui, "cc-export", &mut f.representation, &[("plan", "Plan"), ("engineering", "Engineering"), ("model3d", "3D")]);
            choice(
                ui,
                "cc-paper",
                &mut f.paper,
                &[("ANSI D", "ANSI D"), ("ANSI C", "ANSI C"), ("ANSI B", "ANSI B"), ("Letter", "Letter"), ("A3", "A3"), ("A4", "A4")],
            );
            ui.horizontal(|ui| {
                ui.label("Scale 1:");
                ui.add(DragValue::new(&mut f.denominator).range(1.0..=10000.0));
            });
            if ui.button("Export PDF / DXF / DWG").clicked() {
                let p = json!({"path":f.path,"representation":f.representation,"paper":f.paper,"scale":1.0/f.denominator,"fit":false});
                run(app, "contractor.export", p);
            }
        });
        ui.separator();
        if ui.button("Check collisions").clicked()
            && let Some(v) = run(app, "contractor.collisions", json!({}))
            && let Ok(st) = app.session.state()
        {
            app.contractor.forms.report = Some((st.uid, st.revision, v));
        }
        if let Some((uid, rev, report)) = &app.contractor.forms.report {
            if app.session.state().is_ok_and(|s| s.uid == *uid && s.revision == *rev) {
                ui.label(report.get("message").and_then(Value::as_str).unwrap_or(""));
                let pairs = report.get("overlaps").and_then(Value::as_array).cloned().unwrap_or_default();
                for pair in pairs.iter().take(50) {
                    if ui.button(format!("Select {} / {}", pair["a"], pair["b"])).clicked() {
                        run(app, "select", json!({"handles":[pair["a"],pair["b"]]}));
                    }
                }
            } else {
                ui.small("Model changed; check collisions again.");
            }
        }
        if let Some(e) = &app.contractor.forms.error {
            ui.colored_label(crate::theme::Tokens::get().warn, e);
        }
        ui.separator();
        if ui.button("Add sample floor").clicked() {
            run(app, "contractor.sample", json!({}));
            let _ = app.session.zoom_extents();
        }
    });
}

fn kind(m: &Member) -> &'static str {
    match m.record.shape {
        Shape::Board { .. } => "lumber",
        Shape::Post { .. } => "post",
        Shape::Pile { .. } => "pile",
    }
}
pub fn member_type(d: &cadcraft_doc::Drawing, h: Handle) -> Option<&'static str> {
    model::resolve(d, h).ok().map(|m| title(kind(&m)))
}
fn values(m: &Member) -> BTreeMap<String, String> {
    let mut v = BTreeMap::new();
    v.insert("label".into(), m.record.label.clone());
    v.insert("role".into(), m.record.role.clone());
    v.insert("size".into(), m.record.profile.clone());
    let (width, depth) = match m.record.shape {
        Shape::Board { width, depth, .. } => (width * m.insert.scale.y.abs(), depth * m.insert.scale.z.abs()),
        Shape::Post { width, depth, .. } => (width * m.insert.scale.x.abs(), depth * m.insert.scale.y.abs()),
        Shape::Pile { diameter, .. } => (diameter * m.insert.scale.x.abs(), diameter * m.insert.scale.x.abs()),
    };
    v.insert("flat".into(), (width > depth).to_string());
    for (k, n) in [
        ("width", width),
        ("depth", depth),
        ("diameter", width),
        ("height", m.top() - m.bottom()),
        ("depthBelowTop", m.top() - m.bottom()),
        ("bottom", m.bottom()),
        ("top", m.top()),
        ("startSetback", m.record.setbacks.start * m.insert.scale.x.abs()),
        ("endSetback", m.record.setbacks.end * m.insert.scale.x.abs()),
    ] {
        v.insert(k.into(), n.to_string());
    }
    if let Shape::Board { length, .. } = m.record.shape {
        v.insert("length".into(), (length * m.insert.scale.x.abs()).to_string());
    }
    v
}
fn draft(members: &[Member]) -> Option<Draft> {
    let first = members.first()?;
    let mut common = values(first);
    let mut mixed = BTreeSet::new();
    let mut ty = kind(first);
    for m in members.iter().skip(1) {
        let next = values(m);
        for (k, v) in &mut common {
            if next.get(k) != Some(v) {
                mixed.insert(k.clone());
                v.clear();
            }
        }
        if ty != kind(m) {
            ty = if ty != "lumber" && ty != "mixed" && kind(m) != "lumber" { "support" } else { "mixed" };
        }
    }
    Some(Draft { kind: ty.into(), values: common, dirty: BTreeSet::new(), mixed })
}

/// Return true only when this adapter owns the displayed Properties body.
pub fn properties(app: &mut CadApp, ui: &mut egui::Ui) -> bool {
    sync(app);
    let hs = app.session.selection();
    if hs.is_empty() {
        if app.ui.toolset_tab != "Contractor" {
            return false;
        }
        let f = &mut app.contractor.forms;
        let Some(d) = f.drafts.get_mut(&f.kind) else { return false };
        ui.heading(format!("New {}", title(&d.kind).to_lowercase()));
        editor(ui, d, false);
        let p = d.params(false);
        if ui.button("Choose base…").clicked() && run(app, "contractor.tool", p).is_some() {
            app.contractor.forms.pending_base = true;
            app.start("contractor.base");
        }
        if ui.button("Place item").clicked() {
            place(app);
        }
        reference_choices(app, ui);
        return true;
    }
    if hs.len() > 10000 {
        return false;
    }
    let Ok(st) = app.session.state() else { return false };
    let members = hs.iter().map(|h| model::resolve(&st.doc, *h)).collect::<model::Result<Vec<_>>>();
    let Ok(members) = members else { return false };
    let (uid, revision) = (st.uid, st.revision);
    let f = &mut app.contractor.forms;
    if f.edit.as_ref().is_none_or(|e| e.uid != uid || e.revision != revision || e.handles != hs) {
        let Some(draft) = draft(&members) else { return false };
        let cut = members.first().filter(|_| members.len() == 1).and_then(|m| m.record.cut_length().map(|l| l * m.insert.scale.x.abs()));
        f.edit = Some(Edit { uid, revision, handles: hs.clone(), draft, cut });
    }
    let Some(e) = f.edit.as_mut() else { return false };
    ui.heading(format!("{} ({})", title(&e.draft.kind), hs.len()));
    if hs.len() > 1 {
        ui.small("Varies means different values. Only edited fields will change.");
    }
    editor(ui, &mut e.draft, true);
    if let Some(cut) = e.cut {
        ui.label(format!("Physical cut length: {cut:.4}"));
    }
    let mut p = e.draft.params(true);
    p["handles"] = json!(hs.iter().map(|h| h.hex()).collect::<Vec<_>>());
    let lumber = e.draft.kind == "lumber";
    let changed = !e.draft.dirty.is_empty();
    if ui.add_enabled(changed, egui::Button::new("Apply member changes")).clicked() {
        run(app, "contractor.set", p);
    }
    if ui.button("Choose base…").clicked() {
        app.start("contractor.base");
    }
    if lumber && hs.len() == 1 && ui.button("Butt end to member…").clicked() {
        app.start("contractor.join");
    }
    reference_choices(app, ui);
    if let Some(e) = &app.contractor.forms.error {
        ui.colored_label(crate::theme::Tokens::get().warn, e);
    }
    // Append the host's normal General/Geometry fields for selected objects, so layer,
    // color and other ordinary CAD properties remain available below member properties.
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inspect(app: &mut CadApp) {
        let ctx = egui::Context::default();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            properties(app, ui);
        });
        frame.textures_delta.clear();
    }
    #[test]
    fn property_draft_tracks_drawing_identity_and_selection() {
        let mut s = cadcraft_engine::Session::new();
        s.execute("contractor.board", &json!({"start":[0,0],"end":[120,0]})).unwrap();
        let mut app = CadApp::new(s, crate::Services::default());
        inspect(&mut app);
        let old = app.contractor.forms.edit.as_ref().unwrap();
        let uid = old.uid;
        assert_eq!(old.draft.values["size"], "2x10");
        app.session.new_drawing(false);
        app.session.execute("contractor.board", &json!({"start":[0,0],"end":[120,0],"size":"2x12"})).unwrap();
        inspect(&mut app);
        let new = app.contractor.forms.edit.as_ref().unwrap();
        assert_ne!(uid, new.uid);
        assert_eq!(new.draft.values["size"], "2x12");
    }
    #[test]
    fn mixed_depths_are_not_overwritten_by_an_unrelated_property() {
        let mut s = cadcraft_engine::Session::new();
        let mut hs = Vec::new();
        for bottom in [-96.0, -120.0] {
            let v = s.execute("contractor.post", &json!({"at":[0,0],"shape":"pile","bottom":bottom,"top":0})).unwrap();
            hs.push(v["handle"].as_str().unwrap().to_string());
        }
        s.execute("select", &json!({"handles":hs})).unwrap();
        let mut app = CadApp::new(s, crate::Services::default());
        inspect(&mut app);
        let e = app.contractor.forms.edit.as_mut().unwrap();
        assert!(e.draft.values["depthBelowTop"].is_empty());
        e.draft.values.insert("label".into(), "Foundation".into());
        e.draft.dirty.insert("label".into());
        let mut p = e.draft.params(true);
        p["handles"] = json!(hs);
        app.run("contractor.set", p).unwrap();
        let members = model::members(app.session.doc().unwrap());
        assert_eq!(members[0].bottom(), -96.0);
        assert_eq!(members[1].bottom(), -120.0);
    }
}
