//! Thin command adapters; geometry and persistent data belong to cadcraft-contractor.
use super::{bad, point_req, targets};
use crate::{Accept, CommandSpec, EngineError, Input, Interactive, Prompt, Result, Session, Step};
use cadcraft_contractor::{self as framing, MemberRecord, Representation, Shape};
use cadcraft_doc::{Drawing, EntityKind, Handle, Space};
use cadcraft_geom::{Vec2, Vec3};
use serde_json::{Value, json};

pub use cadcraft_contractor as model;

fn err(e: impl std::fmt::Display) -> EngineError {
    bad("contractor", e.to_string())
}
fn model_space(s: &Session) -> std::result::Result<(), String> {
    if s.space() != Space::Model {
        return Err("Contractor members belong in model space".into());
    }
    s.doc().map(|_| ()).map_err(|e| e.to_string())
}
fn number(p: &Value, key: &str, default: f64) -> Result<f64> {
    match p.get(key) {
        None => Ok(default),
        Some(v) => v.as_f64().filter(|v| v.is_finite()).ok_or_else(|| err(format!("{key} must be a finite number"))),
    }
}
/// Bare values use drawing units; explicit feet/inches and metric suffixes are physical lengths.
pub fn parse_length(d: &Drawing, value: &str) -> Result<f64> {
    if value.len() > 128 {
        return Err(err("length input is too long"));
    }
    let value = value.trim().to_ascii_lowercase();
    let mm = framing::unit_mm(d).map_err(err)?;
    let (text, factor) = if value.contains(['\'', '"']) {
        (value.as_str(), 25.4 / mm)
    } else {
        let suffix = [("mm", 1.0), ("cm", 10.0), ("in", 25.4), ("ft", 304.8), ("m", 1000.0)].into_iter().find(|(s, _)| value.ends_with(s));
        match suffix {
            Some((s, f)) => (value.strip_suffix(s).unwrap_or("").trim(), f / mm),
            None => (value.as_str(), 1.0),
        }
    };
    let n = crate::units::parse_distance(text).ok_or_else(|| err("enter a length such as 9.25\", 1.2', or 250 mm"))? * factor;
    if !n.is_finite() || n.abs() > 1e9 {
        return Err(err("length must be finite and within drawing limits"));
    }
    Ok(n)
}
fn dimensions(d: &Drawing, p: &Value) -> Result<Value> {
    let mut p = p.clone();
    for key in ["width", "depth", "height", "diameter", "length", "bottom", "top", "spacing", "startSetback", "endSetback", "gap", "depthBelowTop"] {
        if let Some(v) = p.get(key).and_then(Value::as_str) {
            p[key] = json!(parse_length(d, v)?);
        }
    }
    Ok(p)
}
fn text<'a>(p: &'a Value, key: &str, default: &'a str) -> Result<&'a str> {
    match p.get(key) {
        None => Ok(default),
        Some(v) => v.as_str().filter(|s| s.len() <= 128 && !s.chars().any(char::is_control)).ok_or_else(|| err(format!("invalid {key}"))),
    }
}
fn flag(p: &Value, key: &str, default: bool) -> Result<bool> {
    match p.get(key) {
        None => Ok(default),
        Some(v) => v.as_bool().ok_or_else(|| err(format!("{key} must be a boolean"))),
    }
}
fn bottom(d: &Drawing, p: &Value) -> Result<f64> {
    if let Some(v) = p.get("onTopOf") {
        let h = v.as_str().and_then(Handle::parse_hex).ok_or_else(|| err("onTopOf requires a member handle"))?;
        return framing::resolve(d, h).map(|m| m.top()).map_err(err);
    }
    number(p, "bottom", 0.0)
}
fn board_record(d: &Drawing, p: &Value, length: f64) -> Result<MemberRecord> {
    let size = text(p, "size", "2x10")?;
    let flat = flag(p, "flat", false)?;
    let (width, depth) = if size == "custom" {
        let (w, d) = (number(p, "width", 1.5)?, number(p, "depth", 9.25)?);
        if flat { (d, w) } else { (w, d) }
    } else {
        framing::lumber(size, framing::unit_mm(d).map_err(err)?, flat).map_err(err)?
    };
    let mut r = MemberRecord::wood(size, text(p, "role", "joist")?, Shape::Board { length, width, depth });
    r.label = text(p, "label", "")?.into();
    r.validate().map_err(err)?;
    Ok(r)
}
fn board_spec(d: &Drawing, p: &Value) -> Result<(MemberRecord, Vec3, f64)> {
    let a = point_req("contractor.board", p, "start")?;
    let b = point_req("contractor.board", p, "end")?;
    let r = board_record(d, p, a.dist(b))?;
    let offset = match text(p, "alignment", "center")? {
        "center" => 0.0,
        "left" => -r.shape.width() / 2.0,
        "right" => r.shape.width() / 2.0,
        _ => return Err(err("alignment must be center, left, or right")),
    };
    let at = a + (b - a).normalized().perp() * offset;
    Ok((r, at.to3(bottom(d, p)?), a.angle_to(b)))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("contractor.tool","Framing Tool Settings",tool).params("{size?: 2x2|2x4|2x6|2x8|2x10|2x12|custom, width?, depth?, flat?, bottom?, onTopOf?: hex, role?, label?, height?, diameter?, shape?: post|pile}").noundo(),
        CommandSpec::new("contractor.board","Place Lumber",board).menu(&["Tools","Contractor","Place Lumber"]).alias(&["ccboard"]).params("{start:[x,y], end:[x,y], size?: 2x10, flat?:false, width?, depth? (custom), bottom?:0, onTopOf?:hex, alignment?:center|left|right, role?:joist, label?}").enabled(model_space).interactive(|s| Ok(Box::new(BoardM {params:s.state()?.contractor_tool.clone(),a:None}))),
        CommandSpec::new("contractor.post","Place Post / Piling",post).menu(&["Tools","Contractor","Place Post / Piling"]).alias(&["ccpost"]).params("{at:[x,y], shape?:post|pile, width?, depth?, diameter?, bottom?:0, top? | height?:96, role?, label?}").enabled(model_space).interactive(|s| Ok(Box::new(PostM(s.state()?.contractor_tool.clone())))),
        CommandSpec::new("contractor.convert","Convert Selected Lines / Circles",convert).menu(&["Tools","Contractor","Convert Selected Lines / Circles"]).params("{handles?:[hex], size?, flat?, width?, depth?, bottom?, top?|height?, alignment?:center|left|right, replace?:false, role?}").enabled(model_space),
        CommandSpec::new("contractor.repeat","Repeat at On-center Spacing",repeat).menu(&["Tools","Contractor","Repeat at On-center Spacing"]).params("{handle:hex, count:2..10000 (includes original), spacing:number, direction?:[x,y] (default perpendicular)}").enabled(model_space).interactive(|_|Ok(Box::new(RepeatM {handle:None,count:None}))),
        CommandSpec::new("contractor.set","Edit Framing Member",set).params("{handle?:hex | handles?:[hex], size?, flat?, shape?:post|pile, width?, depth?, length?, height?, diameter?, bottom?, top?, depthBelowTop?, onTopOf?, start?, end?, startSetback?, endSetback?, role?, label?} (lengths: numbers in drawing units or physical unit strings; atomic batch)").enabled(model_space),
        CommandSpec::new("contractor.base","Choose Member Base",base).menu(&["Tools","Contractor","Choose Member Base"]).params("{handles?:[hex], reference:hex, side?:above|below} (empty selection sets placement defaults)").enabled(model_space).interactive(|_|Ok(Box::new(ReferenceM::new(false)))),
        CommandSpec::new("contractor.join","Butt Lumber End",join).menu(&["Tools","Contractor","Butt Lumber End"]).params("{handle:hex, reference:hex, end?:auto|start|end, gap?:0}").enabled(model_space).interactive(|_|Ok(Box::new(ReferenceM::new(true)))),
        CommandSpec::new("contractor.collisions","Check Framing Collisions",collisions).menu(&["Tools","Contractor","Check Framing Collisions"]).params("{} -> {overlaps:[{a,b}]} (positive physical volume only)").noundo(),
        CommandSpec::new("contractor.list","Framing Members",list).params("{} -> {members, issues}").noundo(),
        CommandSpec::new("contractor.view","Framing View",view).params("{mode?:plan|engineering|model3d} -> {mode}"),
        CommandSpec::new("contractor.view.plan","Plan View",|s,_|view(s,&json!({"mode":"plan"}))).menu(&["Tools","Contractor","Plan View"]),
        CommandSpec::new("contractor.view.engineering","Engineering View",|s,_|view(s,&json!({"mode":"engineering"}))).menu(&["Tools","Contractor","Engineering View"]),
        CommandSpec::new("contractor.view.model3d","3D View",|s,_|view(s,&json!({"mode":"model3d"}))).menu(&["Tools","Contractor","3D View"]),
        CommandSpec::new("contractor.export","Export Framing Drawing",export).menu(&["Tools","Contractor","Export Framing Drawing"]).params("{path:PDF|DXF|DWG, representation?:plan|engineering|model3d, paper?:ANSI D, scale?:0.0208333333, fit?:false, landscape?:true}").noundo().interactive(|_|Ok(Box::new(ExportM))),
        CommandSpec::new("contractor.sample","Sample Floor Framing",sample).menu(&["Tools","Contractor","Sample Floor Framing"]).params("{} (adds synthetic inch-based floor framing at the origin)").enabled(model_space),
    ]
}

fn tool(s: &mut Session, p: &Value) -> Result<Value> {
    let p = &dimensions(s.doc()?, p)?;
    if !p.is_object() {
        return Err(err("tool settings must be an object"));
    }
    let allowed = [
        "kind",
        "size",
        "width",
        "depth",
        "flat",
        "bottom",
        "onTopOf",
        "role",
        "label",
        "height",
        "top",
        "diameter",
        "shape",
        "alignment",
        "depthBelowTop",
    ];
    if p.as_object().is_some_and(|o| o.keys().any(|k| !allowed.contains(&k.as_str()))) {
        return Err(err("unknown framing tool setting"));
    }
    let old = s.state()?.contractor_tool.clone();
    let mut next = if p.get("kind").is_some() && p.get("kind") != old.get("kind") { json!({}) } else { old };
    let obj = next.as_object_mut().ok_or_else(|| err("invalid tool settings"))?;
    if p.get("bottom").is_some() || p.get("onTopOf") == Some(&Value::Null) {
        obj.remove("onTopOf");
    }
    if p.get("height").is_some() {
        obj.remove("top");
        obj.remove("depthBelowTop");
    }
    for (key, v) in p.as_object().into_iter().flatten() {
        obj.insert(key.clone(), v.clone());
    }
    if obj.get("onTopOf") == Some(&Value::Null) {
        obj.remove("onTopOf");
    }
    board_record(s.doc()?, &next, 1.0)?;
    if !["lumber", "post", "pile"].contains(&text(&next, "kind", "lumber")?) {
        return Err(err("invalid placement type"));
    }
    bottom(s.doc()?, &next)?;
    post_spec(s.doc()?, &next, Vec2::ZERO)?;
    s.state_mut()?.contractor_tool = next.clone();
    Ok(next)
}

fn board(s: &mut Session, p: &Value) -> Result<Value> {
    let p = &dimensions(s.doc()?, p)?;
    let (r, at, angle) = board_spec(s.doc()?, p)?;
    let common = s.current_common();
    let mut next = s.doc()?.clone();
    let h = framing::add(&mut next, r, at, angle, common).map_err(err)?;
    *s.doc_mut()? = next;
    s.set_selection(vec![h]);
    Ok(json!({"handle":h.hex()}))
}
fn post_spec(d: &Drawing, p: &Value, at: Vec2) -> Result<(MemberRecord, Vec3)> {
    let z = if p.get("depthBelowTop").is_some() { number(p, "top", 0.0)? - number(p, "depthBelowTop", 0.0)? } else { bottom(d, p)? };
    let height =
        if p.get("top").is_some() { number(p, "top", 0.0)? - z } else { number(p, "height", 96.0 * 25.4 / framing::unit_mm(d).map_err(err)?)? };
    let shape = match text(p, "shape", "post")? {
        "post" => Shape::Post {
            width: number(p, "width", 5.5 * 25.4 / framing::unit_mm(d).map_err(err)?)?,
            depth: number(p, "depth", 5.5 * 25.4 / framing::unit_mm(d).map_err(err)?)?,
            height,
        },
        "pile" => Shape::Pile { diameter: number(p, "diameter", 10.0 * 25.4 / framing::unit_mm(d).map_err(err)?)?, height },
        _ => return Err(err("shape must be post or pile")),
    };
    let mut r = MemberRecord::wood("custom", text(p, "role", if matches!(shape, Shape::Pile { .. }) { "piling" } else { "post" })?, shape);
    r.label = text(p, "label", "")?.into();
    r.validate().map_err(err)?;
    Ok((r, at.to3(z)))
}
fn post(s: &mut Session, p: &Value) -> Result<Value> {
    let p = &dimensions(s.doc()?, p)?;
    let (r, at) = post_spec(s.doc()?, p, point_req("contractor.post", p, "at")?)?;
    let common = s.current_common();
    let mut next = s.doc()?.clone();
    let h = framing::add(&mut next, r, at, 0.0, common).map_err(err)?;
    *s.doc_mut()? = next;
    s.set_selection(vec![h]);
    Ok(json!({"handle":h.hex()}))
}

fn convert(s: &mut Session, p: &Value) -> Result<Value> {
    let p = &dimensions(s.doc()?, p)?;
    if !p.is_object() {
        return Err(err("conversion parameters must be an object"));
    }
    let hs = targets(s, p)?;
    if hs.is_empty() || hs.len() > framing::MAX_REPEAT {
        return Err(err("select 1..10000 straight lines or circles"));
    }
    let replace = flag(p, "replace", false)?;
    let mut next = s.doc()?.clone();
    let mut out = Vec::new();
    for h in hs {
        let e = next.entity(h).cloned().ok_or_else(|| err("selected object does not exist"))?;
        if next.space_of(h) != Some(Space::Model) || next.layer(&e.common.layer).is_some_and(|l| l.locked) {
            return Err(err("conversion requires unlocked model-space objects"));
        }
        let (r, at, angle) = match &e.kind {
            EntityKind::Line(l) => {
                if (l.a.z - l.b.z).abs() > 1e-8 {
                    return Err(err("sloped lines are not supported in floor framing v1"));
                }
                let mut options = p.clone();
                options["start"] = json!([l.a.x, l.a.y]);
                options["end"] = json!([l.b.x, l.b.y]);
                if p.get("bottom").is_none() && p.get("onTopOf").is_none() {
                    options["bottom"] = json!(l.a.z);
                }
                board_spec(&next, &options)?
            }
            EntityKind::Circle(c) => {
                let mut options = p.clone();
                options["shape"] = json!("pile");
                options["diameter"] = json!(c.radius * 2.0);
                options["role"] = json!("piling");
                if p.get("bottom").is_none() && p.get("onTopOf").is_none() {
                    options["bottom"] = json!(c.center.z);
                }
                let (r, at) = post_spec(&next, &options, c.center.xy())?;
                (r, at, 0.0)
            }
            _ => return Err(err("conversion accepts only straight LINEs and CIRCLEs")),
        };
        out.push(framing::add(&mut next, r, at, angle, e.common.clone()).map_err(err)?);
        if replace {
            next.remove_entity(h);
        }
    }
    *s.doc_mut()? = next;
    s.set_selection(out.clone());
    Ok(json!({"handles":out.iter().map(|h|h.hex()).collect::<Vec<_>>()}))
}

fn single(s: &Session, p: &Value) -> Result<Handle> {
    let hs = targets(s, p)?;
    if hs.len() != 1 {
        return Err(err("select exactly one framing member"));
    }
    hs.first().copied().ok_or_else(|| err("member handle required"))
}
fn editable(s: &Session, h: Handle) -> Result<framing::Member> {
    if s.doc()?.entity(h).and_then(|e| s.doc().ok()?.layer(&e.common.layer)).is_some_and(|l| l.locked) {
        return Err(err("member is on a locked layer"));
    }
    framing::resolve(s.doc()?, h).map_err(err)
}
fn repeat(s: &mut Session, p: &Value) -> Result<Value> {
    let p = &dimensions(s.doc()?, p)?;
    let h = single(s, p)?;
    let m = editable(s, h)?;
    let count = p
        .get("count")
        .and_then(Value::as_u64)
        .filter(|n| (2..=framing::MAX_REPEAT as u64).contains(n))
        .ok_or_else(|| err("count must be 2..10000, including the original"))?;
    let spacing = number(p, "spacing", 0.0)?;
    if spacing <= 0.0 {
        return Err(err("spacing must be positive"));
    }
    let dir = if p.get("direction").is_some() {
        point_req("contractor.repeat", p, "direction")?
    } else if matches!(m.record.shape, Shape::Board { .. }) {
        let (a, b) = m.axis();
        (b.xy() - a.xy()).normalized().perp()
    } else {
        Vec2::from_angle(m.insert.rotation).perp()
    };
    let magnitude = dir.x.hypot(dir.y);
    if !magnitude.is_finite() || magnitude < 1e-8 {
        return Err(err("repeat direction must be nonzero"));
    }
    let delta = (dir / magnitude) * spacing;
    let mut next = s.doc()?.clone();
    let e = next.entity(h).cloned().ok_or_else(|| err("no such member"))?;
    let mut out = vec![h];
    for n in 1..count {
        let at = (m.insert.insert.xy() + delta * n as f64).to3(m.bottom());
        let mut record = m.record.clone();
        // Normalize source scales into physical dimensions before creating independent copies.
        scale_shape(&mut record.shape, m.insert.scale);
        record.setbacks.start *= m.insert.scale.x.abs();
        record.setbacks.end *= m.insert.scale.x.abs();
        let angle =
            m.insert.rotation + if matches!(record.shape, Shape::Board { .. }) && m.insert.scale.x < 0.0 { std::f64::consts::PI } else { 0.0 };
        out.push(framing::add(&mut next, record, at, angle, e.common.clone()).map_err(err)?);
    }
    *s.doc_mut()? = next;
    s.set_selection(out.clone());
    Ok(json!({"handles":out.iter().map(|h|h.hex()).collect::<Vec<_>>()}))
}
fn scale_shape(shape: &mut Shape, scale: Vec3) {
    match shape {
        Shape::Board { length, width, depth } => {
            *length *= scale.x.abs();
            *width *= scale.y.abs();
            *depth *= scale.z.abs();
        }
        Shape::Post { width, depth, height } => {
            *width *= scale.x.abs();
            *depth *= scale.y.abs();
            *height *= scale.z.abs();
        }
        Shape::Pile { diameter, height } => {
            *diameter *= scale.x.abs();
            *height *= scale.z.abs();
        }
    }
}
fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let p = &dimensions(s.doc()?, p)?;
    let hs = targets(s, p)?;
    if hs.is_empty() || hs.len() > framing::MAX_REPEAT {
        return Err(err("select 1..10000 framing members"));
    }
    let mut next = s.doc()?.clone();
    for h in hs {
        let m = editable(s, h)?;
        set_member(&mut next, h, m, p)?;
    }
    *s.doc_mut()? = next;
    list(s, &json!({}))
}
fn set_member(d: &mut Drawing, h: Handle, m: framing::Member, p: &Value) -> Result<()> {
    let mut r = m.record.clone();
    scale_shape(&mut r.shape, m.insert.scale);
    r.setbacks.start *= m.insert.scale.x.abs();
    r.setbacks.end *= m.insert.scale.x.abs();
    let mut at = m.insert.insert;
    at.z = m.bottom();
    let mut angle = m.insert.rotation;
    if matches!(r.shape, Shape::Board { .. }) && m.insert.scale.x < 0.0 {
        angle += std::f64::consts::PI;
    }
    if p.get("bottom").is_some() || p.get("onTopOf").is_some() {
        at.z = bottom(d, p)?;
    }
    r.role = text(p, "role", &r.role)?.into();
    r.label = text(p, "label", &r.label)?.into();
    if p.get("shape").is_some() && !matches!(r.shape, Shape::Board { .. }) {
        let height = r.shape.height();
        let width = r.shape.width();
        r.shape = match text(p, "shape", "")? {
            "pile" => Shape::Pile { diameter: number(p, "diameter", width)?, height },
            "post" => Shape::Post { width: number(p, "width", width)?, depth: number(p, "depth", width)?, height },
            _ => return Err(err("support type must be post or pile")),
        };
    }
    match &mut r.shape {
        Shape::Board { length, width, depth } => {
            if p.get("size").is_some() || p.get("flat").is_some() {
                let size = text(p, "size", &r.profile)?;
                if size != "custom" {
                    let was_flat = *width > *depth;
                    (*width, *depth) = framing::lumber(size, framing::unit_mm(d).map_err(err)?, flag(p, "flat", was_flat)?).map_err(err)?;
                } else if p.get("flat").is_some()
                    && p.get("width").is_none()
                    && p.get("depth").is_none()
                    && (*width > *depth) != flag(p, "flat", false)?
                {
                    std::mem::swap(width, depth);
                }
                r.profile = size.into();
            }
            *length = number(p, "length", *length)?;
            if p.get("start").is_some() || p.get("end").is_some() {
                let a = if p.get("start").is_some() { point_req("contractor.set", p, "start")? } else { at.xy() };
                let b = if p.get("end").is_some() { point_req("contractor.set", p, "end")? } else { at.xy() + Vec2::from_angle(angle) * *length };
                *length = a.dist(b);
                at = a.to3(at.z);
                angle = a.angle_to(b);
            }
            *width = number(p, "width", *width)?;
            *depth = number(p, "depth", *depth)?;
            if p.get("width").is_some() || p.get("depth").is_some() {
                r.profile = "custom".into();
            }
        }
        Shape::Post { width, depth, height } => {
            *width = number(p, "width", *width)?;
            *depth = number(p, "depth", *depth)?;
            *height = number(p, "height", *height)?;
        }
        Shape::Pile { diameter, height } => {
            *diameter = number(p, "diameter", *diameter)?;
            *height = number(p, "height", *height)?;
        }
    }
    if !matches!(r.shape, Shape::Board { .. }) {
        let mut height = r.shape.height();
        if p.get("top").is_some() {
            height = number(p, "top", m.top())? - at.z;
        }
        if p.get("depthBelowTop").is_some() {
            height = number(p, "depthBelowTop", height)?;
            at.z = number(p, "top", m.top())? - height;
        }
        match &mut r.shape {
            Shape::Post { height: h, .. } | Shape::Pile { height: h, .. } => *h = height,
            _ => {}
        }
    }
    if p.get("startSetback").is_some() || p.get("endSetback").is_some() {
        r.version = 2;
        r.setbacks.start = number(p, "startSetback", r.setbacks.start)?;
        r.setbacks.end = number(p, "endSetback", r.setbacks.end)?;
    }
    framing::update(d, h, r, at, angle).map_err(err)
}
fn list(s: &mut Session, _p: &Value) -> Result<Value> {
    let d = s.doc()?;
    let mut issues = Vec::new();
    for e in d.model.iter().filter(|e| framing::tagged(e)) {
        if let Err(error) = framing::resolve(d, e.handle) {
            issues.push(json!({"handle":e.handle.hex(),"reason":error.to_string()}));
        }
    }
    let members = framing::members(d)
        .into_iter()
        .map(|m| {
            let (a, b) = m.axis();
            json!({"handle":m.handle.hex(),"record":m.record,"bottom":m.bottom(),"top":m.top(),"start":m.insert.insert,"axis":[a,b],"cutLength":m.record.cut_length().map(|l|l*m.insert.scale.x.abs())})
        })
        .collect::<Vec<_>>();
    Ok(json!({"members":members,"issues":issues,"unitMm":framing::unit_mm(d).ok()}))
}

fn reference(s: &Session, p: &Value) -> Result<framing::Member> {
    let h = p.get("reference").and_then(Value::as_str).and_then(Handle::parse_hex).ok_or_else(|| err("reference member handle required"))?;
    framing::resolve(s.doc()?, h).map_err(err)
}
fn base(s: &mut Session, p: &Value) -> Result<Value> {
    let reference = reference(s, p)?;
    let above = match text(p, "side", "above")? {
        "above" => true,
        "below" => false,
        _ => return Err(err("side must be above or below")),
    };
    let hs = targets(s, p)?;
    if hs.is_empty() {
        let defaults = s.state()?.contractor_tool.clone();
        let height = if defaults.get("kind").and_then(Value::as_str) == Some("lumber") || defaults.get("shape").is_none() {
            board_record(s.doc()?, &defaults, 1.0)?.shape.height()
        } else {
            post_spec(s.doc()?, &defaults, Vec2::ZERO)?.0.shape.height()
        };
        let z = if above { reference.top() } else { reference.bottom() - height };
        // tool.kind is an adapter preference; the command validates the physical settings.
        let mut next = defaults;
        next["bottom"] = json!(z);
        if next.get("depthBelowTop").is_some() {
            next["top"] = json!(z + height);
        }
        if let Some(o) = next.as_object_mut() {
            o.remove("onTopOf");
        }
        s.state_mut()?.contractor_tool = next;
        return Ok(json!({"bottom":z}));
    }
    if hs.len() > framing::MAX_REPEAT {
        return Err(err("too many members"));
    }
    let mut next = s.doc()?.clone();
    for h in &hs {
        if *h == reference.handle {
            return Err(err("a member cannot be its own base"));
        }
        let m = editable(s, *h)?;
        let z = if above { reference.top() } else { reference.bottom() - (m.top() - m.bottom()) };
        set_member(&mut next, *h, m, &json!({"bottom":z}))?;
    }
    *s.doc_mut()? = next;
    s.set_selection(hs);
    list(s, &json!({}))
}
fn join(s: &mut Session, p: &Value) -> Result<Value> {
    let p = &dimensions(s.doc()?, p)?;
    let h = single(s, p)?;
    let m = editable(s, h)?;
    let r = reference(s, p)?;
    let edit = framing::joints::butt(&m, &r, text(p, "end", "auto")?, number(p, "gap", 0.0)?).map_err(err)?;
    framing::update(s.doc_mut()?, h, edit.record, edit.at, edit.rotation).map_err(err)?;
    list(s, &json!({}))
}
fn collisions(s: &mut Session, _p: &Value) -> Result<Value> {
    let overlaps = framing::joints::collisions(s.doc()?).map_err(err)?.into_iter().map(|(a, b)| json!({"a":a.hex(),"b":b.hex()})).collect::<Vec<_>>();
    Ok(json!({"message":format!("{} physical overlaps found.",overlaps.len()),"overlaps":overlaps}))
}
fn mode(p: &Value) -> Result<Representation> {
    match text(p, "mode", "plan")? {
        "plan" => Ok(Representation::Plan),
        "engineering" => Ok(Representation::Engineering),
        "model3d" | "3d" => Ok(Representation::Model3d),
        _ => Err(err("mode must be plan, engineering, or model3d")),
    }
}
fn view(s: &mut Session, p: &Value) -> Result<Value> {
    if p.get("mode").is_some() {
        let rep = mode(p)?;
        framing::set_representation(s.doc_mut()?, rep);
        if rep == Representation::Engineering
            && s.space() == Space::Model
            && let Ok(mm) = framing::unit_mm(s.doc()?)
            && let Ok(d) = framing::export_drawing(s.doc()?, rep, 2.5 / mm * 48.0)
        {
            let bounds = d.extents(&Space::Model);
            if !bounds.is_empty() {
                let center = bounds.center();
                let aspect = s.viewport_px.0.max(1.0) / s.viewport_px.1.max(1.0);
                let height = (bounds.height().max(bounds.width() / aspect) * 1.1).max(1e-6);
                s.execute("view.set", &json!({"center":[center.x,center.y],"height":height}))?;
            }
        }
    }
    Ok(json!({"mode":framing::representation(s.doc()?)}))
}
fn export(s: &mut Session, p: &Value) -> Result<Value> {
    let path = p.get("path").and_then(Value::as_str).filter(|s| !s.chars().any(char::is_control)).ok_or_else(|| err("export path is required"))?;
    if path.is_empty() || path.len() > 4096 {
        return Err(err("export path is required"));
    }
    let rep = mode(&json!({"mode":text(p,"representation","plan")?}))?;
    let scale = number(p, "scale", 1.0 / 48.0)?;
    if scale <= 0.0 || scale > 1e6 {
        return Err(err("plot scale must be positive and at most 1e6"));
    }
    let mm = framing::unit_mm(s.doc()?).map_err(err)?;
    let d = framing::export_drawing(s.doc()?, rep, 2.5 / mm / scale).map_err(err)?;
    let ext = std::path::Path::new(path).extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
    let io = super::file::io().ok_or_else(|| err("file writer is not available"))?;
    let bytes = match ext.as_str() {
        "dxf" | "dwg" => (io.write)(&d, path).map_err(err)?,
        "pdf" => {
            if rep == Representation::Model3d {
                return Err(err("PDF blueprints use plan or engineering representation"));
            }
            let paper_unit_mm = if d.header.i64("MEASUREMENT", 0) == 1 { 1.0 } else { 25.4 };
            let options = json!({"format":"pdf","paper":text(p,"paper","ANSI D")?,"scale":scale*mm/paper_unit_mm,"fit":flag(p,"fit",false)?,"landscape":flag(p,"landscape",true)?});
            (io.plot.ok_or_else(|| err("PDF plotting is not available"))?)(&d, &Space::Model, &options).map_err(err)?
        }
        _ => return Err(err("export supports PDF, DXF, and DWG")),
    };
    super::file::store(path, &bytes)?;
    Ok(json!({"path":path,"bytes":bytes.len(),"representation":rep}))
}
fn sample(s: &mut Session, _p: &Value) -> Result<Value> {
    let inch = 25.4 / framing::unit_mm(s.doc()?).map_err(err)?;
    let mut next = s.doc()?.clone();
    for x in [0.0, 144.0, 288.0] {
        for y in [0.0, 144.0, 288.0] {
            framing::add(
                &mut next,
                MemberRecord::wood("custom", "piling", Shape::Pile { diameter: 10.0 * inch, height: 96.0 * inch }),
                Vec3::new(x * inch, y * inch, -96.0 * inch),
                0.0,
                Default::default(),
            )
            .map_err(err)?;
        }
        framing::add(
            &mut next,
            MemberRecord::wood("2x12", "beam", Shape::Board { length: 288.0 * inch, width: 1.5 * inch, depth: 11.25 * inch }),
            Vec3::new(x * inch, 0.0, 0.0),
            std::f64::consts::FRAC_PI_2,
            Default::default(),
        )
        .map_err(err)?;
    }
    for n in 0..19 {
        framing::add(
            &mut next,
            MemberRecord::wood("2x10", "joist", Shape::Board { length: 288.0 * inch, width: 1.5 * inch, depth: 9.25 * inch }),
            Vec3::new(0.0, n as f64 * 16.0 * inch, 11.25 * inch),
            0.0,
            Default::default(),
        )
        .map_err(err)?;
    }
    *s.doc_mut()? = next;
    Ok(json!({"message":"Added synthetic 24 ft square floor framing (illustrative geometry, no load analysis)."}))
}

struct BoardM {
    params: Value,
    a: Option<Vec2>,
}
impl Interactive for BoardM {
    fn name(&self) -> &'static str {
        "contractor.board"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        view(s, &json!({"mode":"plan"}))?;
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.a {
            None => Prompt::new("Specify board start", Accept::POINT),
            Some(a) => Prompt::new("Specify board end", Accept::POINT).base(a),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Cancel | Input::Enter => Ok(Step::Cancel),
            Input::Point(p) => {
                if let Some(a) = self.a {
                    let mut params = self.params.clone();
                    params["start"] = json!([a.x, a.y]);
                    params["end"] = json!([p.x, p.y]);
                    board(s, &params)?;
                    Ok(Step::Done)
                } else {
                    self.a = Some(p);
                    Ok(Step::Continue)
                }
            }
            _ => Err(err("pick a plan point")),
        }
    }
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        let Some(a) = self.a else { return Vec::new() };
        let mut p = self.params.clone();
        p["start"] = json!([a.x, a.y]);
        p["end"] = json!([cursor.x, cursor.y]);
        let Ok(d) = s.doc() else { return Vec::new() };
        let Ok((r, at, angle)) = board_spec(d, &p) else { return Vec::new() };
        let mut tmp = Drawing { header: d.header.clone(), ..Default::default() };
        let Ok(h) = framing::add(&mut tmp, r, at, angle, Default::default()) else { return Vec::new() };
        let Ok(m) = framing::resolve(&tmp, h) else { return Vec::new() };
        m.faces().into_iter().take(1).map(|f| EntityKind::Polyline3d(cadcraft_doc::Polyline3d { points: f.to_vec(), closed: true })).collect()
    }
}
struct PostM(Value);
impl Interactive for PostM {
    fn name(&self) -> &'static str {
        "contractor.post"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        view(s, &json!({"mode":"plan"}))?;
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Specify post / piling center", Accept::POINT)
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Cancel | Input::Enter => Ok(Step::Cancel),
            Input::Point(p) => {
                let mut params = self.0.clone();
                params["at"] = json!([p.x, p.y]);
                post(s, &params)?;
                Ok(Step::Done)
            }
            _ => Err(err("pick a plan point")),
        }
    }
}

/// Called by the common affine-transform adapter, before it mutates any selected object.
pub(crate) fn guard_transform(s: &Session, hs: &[Handle], m: &cadcraft_geom::Mat3) -> Result<()> {
    if (m.scale_factor() - 1.0).abs() > 1e-8 && hs.iter().any(|h| s.doc().ok().and_then(|d| d.entity(*h)).is_some_and(|e| framing::tagged(e))) {
        return Err(err("resize framing members through Contractor properties; generic SCALE would invalidate lumber sizes"));
    }
    Ok(())
}
pub(crate) fn protected_block(d: &Drawing, name: &str) -> bool {
    d.model.iter().any(|e| framing::tagged(e) && matches!(&e.kind,EntityKind::Insert(i) if i.block.eq_ignore_ascii_case(name)))
}

pub(crate) fn guard_properties(s: &Session, hs: &[Handle], p: &Value) -> Result<()> {
    if p.get("scale").is_some() && hs.iter().any(|h| s.doc().ok().and_then(|d| d.entity(*h)).is_some_and(|e| framing::tagged(e))) {
        return Err(err("resize framing members through Contractor properties"));
    }
    Ok(())
}

struct RepeatM {
    handle: Option<Handle>,
    count: Option<u64>,
}
impl Interactive for RepeatM {
    fn name(&self) -> &'static str {
        "contractor.repeat"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        view(s, &json!({"mode":"plan"}))?;
        if s.settings.pickfirst
            && let Ok(h) = single(s, &json!({}))
            && editable(s, h).is_ok()
        {
            self.handle = Some(h);
        }
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if self.handle.is_none() {
            Prompt::new("Select one framing member to repeat", Accept::SELECT)
        } else if self.count.is_none() {
            Prompt::new("Number of members including original", Accept::NUMBER).default("2")
        } else {
            Prompt::new("On-center spacing", Accept::NUMBER)
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if matches!(i, Input::Cancel) {
            return Ok(Step::Cancel);
        }
        let Some(handle) = self.handle else {
            match i {
                Input::Enter => return Ok(Step::Cancel),
                Input::Pick(hs) => {
                    if hs.len() == 1
                        && let Some(h) = hs.first()
                    {
                        match editable(s, *h) {
                            Ok(_) => {
                                self.handle = Some(*h);
                                s.remember_selection(&hs);
                                s.set_selection(hs);
                            }
                            Err(e) => s.echo(e.to_string()),
                        }
                    } else {
                        s.echo("Choose exactly one framing member.");
                    }
                }
                _ => {}
            }
            return Ok(Step::Continue);
        };
        if self.count.is_none() {
            let count = match i {
                Input::Enter => 2.0,
                Input::Text(t) => t.parse::<f64>().map_err(|_| err("enter an integer count"))?,
                _ => return Err(err("enter an integer count")),
            };
            if !count.is_finite() || count.fract() != 0.0 || !(2.0..=framing::MAX_REPEAT as f64).contains(&count) {
                return Err(err("count must be 2..10000"));
            }
            self.count = Some(count as u64);
            Ok(Step::Continue)
        } else {
            let Input::Text(t) = i else {
                return Err(err("enter positive on-center spacing"));
            };
            let spacing = super::machines::number(&t).ok_or_else(|| err("enter positive on-center spacing"))?;
            repeat(s, &json!({"handle":handle.hex(),"count":self.count,"spacing":spacing}))?;
            Ok(Step::Done)
        }
    }
}
struct ReferenceM {
    join: bool,
    handles: Vec<Handle>,
    reference: Option<Handle>,
}
impl ReferenceM {
    fn new(join: bool) -> Self {
        Self { join, handles: Vec::new(), reference: None }
    }
}
impl Interactive for ReferenceM {
    fn name(&self) -> &'static str {
        if self.join { "contractor.join" } else { "contractor.base" }
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        let selected = s.selection();
        if selected.len() <= framing::MAX_REPEAT && selected.iter().all(|h| editable(s, *h).is_ok()) {
            self.handles = selected;
        }
        if self.join && self.handles.len() != 1 {
            self.handles.clear();
        }
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if self.join && self.handles.is_empty() {
            Prompt::new("Select lumber to join", Accept::SELECT)
        } else if self.reference.is_none() {
            Prompt::new("Choose reference member", Accept::SELECT)
        } else if self.join {
            Prompt::new("Board end", Accept::TEXT).kw(&["Auto", "Start", "End"]).default("Auto")
        } else {
            Prompt::new("Place relative to reference", Accept::TEXT).kw(&["Above", "Below"]).default("Above")
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if matches!(i, Input::Cancel) {
            return Ok(Step::Cancel);
        }
        if self.reference.is_none() {
            if let Input::Pick(hs) = i
                && hs.len() == 1
                && let Some(h) = hs.first()
            {
                framing::resolve(s.doc()?, *h).map_err(err)?;
                if self.join && self.handles.is_empty() {
                    self.handles = hs;
                } else {
                    self.reference = Some(*h);
                }
            }
            return Ok(Step::Continue);
        }
        let option = match i {
            Input::Enter => {
                if self.join {
                    "auto".to_string()
                } else {
                    "above".to_string()
                }
            }
            Input::Text(t) | Input::Keyword(t) => t.to_ascii_lowercase(),
            _ => return Ok(Step::Continue),
        };
        let mut p = json!({"handles":self.handles.iter().map(|h|h.hex()).collect::<Vec<_>>(),"reference":self.reference.map(|h|h.hex())});
        if self.join {
            p["end"] = json!(option);
            join(s, &p)?;
        } else {
            p["side"] = json!(option);
            base(s, &p)?;
        }
        Ok(Step::Done)
    }
}
struct ExportM;
impl Interactive for ExportM {
    fn name(&self) -> &'static str {
        "contractor.export"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Export file path (.pdf, .dxf, or .dwg)", Accept::TEXT)
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Cancel | Input::Enter => Ok(Step::Cancel),
            Input::Text(path) => {
                let representation = framing::representation(s.doc()?);
                export(s, &json!({"path":path,"representation":representation}))?;
                Ok(Step::Done)
            }
            _ => Ok(Step::Continue),
        }
    }
}
