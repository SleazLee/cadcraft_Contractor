//! Portable Contractor objects. The document stores ordinary INSERTs, not a new entity kind.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod annotations;
pub mod joints;
pub mod view3d;
pub use annotations::export_drawing;

use cadcraft_color::Color;
use cadcraft_doc::{Block, Common, Drawing, Entity, EntityKind, Face3d, Handle, Insert, Line, LwPolyline, RawTag};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use serde::{Deserialize, Serialize};

pub const APP: &str = "CADCRAFT_CONTRACTOR";
pub const PLAN: &str = "CC_CONTRACTOR_PLAN";
pub const AXIS: &str = "CC_CONTRACTOR_AXIS";
pub const MESH: &str = "CC_CONTRACTOR_MESH";
pub const NOTES: &str = "CC_CONTRACTOR_NOTES";
pub const MAX_MEMBERS: usize = 100_000;
pub const MAX_REPEAT: usize = 10_000;
const MAX_METADATA: usize = 8192;
const LIMIT: f64 = 1e9;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Document(#[from] cadcraft_doc::DocError),
}
pub type Result<T> = std::result::Result<T, Error>;
fn invalid(s: impl Into<String>) -> Error {
    Error::Invalid(s.into())
}

/// Nominal names and dry dressed dimensions in inches, PS 20-25 Table 3.
pub const LUMBER: &[(&str, f64, f64)] =
    &[("2x2", 1.5, 1.5), ("2x4", 1.5, 3.5), ("2x6", 1.5, 5.5), ("2x8", 1.5, 7.25), ("2x10", 1.5, 9.25), ("2x12", 1.5, 11.25)];

/// Millimetres per drawing unit. A unitless drawing needs an explicit unit choice.
pub fn unit_mm(d: &Drawing) -> Result<f64> {
    match d.header.i64("INSUNITS", 0) {
        1 => Ok(25.4),
        2 => Ok(304.8),
        4 => Ok(1.0),
        5 => Ok(10.0),
        6 => Ok(1000.0),
        _ => Err(invalid("Contractor requires drawing units: inches, feet, millimetres, centimetres, or metres (INSUNITS).")),
    }
}

pub fn lumber(size: &str, mm: f64, flat: bool) -> Result<(f64, f64)> {
    if !mm.is_finite() || mm <= 0.0 {
        return Err(invalid("invalid drawing unit"));
    }
    let name = size.to_lowercase().replace('×', "x");
    let (_, w, d) = LUMBER.iter().find(|(n, _, _)| *n == name).ok_or_else(|| invalid("choose 2x2, 2x4, 2x6, 2x8, 2x10, 2x12, or custom"))?;
    let factor = 25.4 / mm;
    let (w, d) = (w * factor, d * factor);
    Ok(if flat { (d, w) } else { (w, d) })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Shape {
    Board { length: f64, width: f64, depth: f64 },
    Post { width: f64, depth: f64, height: f64 },
    Pile { diameter: f64, height: f64 },
}

impl Shape {
    pub fn height(&self) -> f64 {
        match *self {
            Self::Board { depth, .. } => depth,
            Self::Post { height, .. } | Self::Pile { height, .. } => height,
        }
    }
    pub fn width(&self) -> f64 {
        match *self {
            Self::Board { width, .. } | Self::Post { width, .. } => width,
            Self::Pile { diameter, .. } => diameter,
        }
    }
    pub fn validate(&self) -> Result<()> {
        let values = match *self {
            Self::Board { length, width, depth } => [length, width, depth],
            Self::Post { width, depth, height } => [width, depth, height],
            Self::Pile { diameter, height } => [diameter, diameter, height],
        };
        if values.iter().any(|x| !x.is_finite() || *x <= 1e-8 || *x > LIMIT) {
            return Err(invalid("member dimensions must be finite, positive, and at most 1e9 drawing units"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemberRecord {
    pub version: u16,
    pub material: String,
    pub profile: String,
    pub role: String,
    pub label: String,
    pub shape: Shape,
    #[serde(default, skip_serializing_if = "EndSetbacks::is_zero")]
    pub setbacks: EndSetbacks,
}

/// Square end cuts measured inward from the engineering endpoints, in drawing units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndSetbacks {
    pub start: f64,
    pub end: f64,
}
impl EndSetbacks {
    fn is_zero(&self) -> bool {
        self.start == 0.0 && self.end == 0.0
    }
}

impl MemberRecord {
    pub fn wood(profile: &str, role: &str, shape: Shape) -> Self {
        Self {
            version: 1,
            material: "wood".into(),
            profile: profile.into(),
            role: role.into(),
            label: String::new(),
            shape,
            setbacks: EndSetbacks::default(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.version, 1 | 2) {
            return Err(invalid("unsupported Contractor metadata version; kept as ordinary CAD geometry"));
        }
        if [&self.material, &self.profile, &self.role, &self.label].iter().any(|s| s.len() > 128 || s.chars().any(char::is_control)) {
            return Err(invalid("member names must be at most 128 bytes without control characters"));
        }
        self.shape.validate()?;
        if [self.setbacks.start, self.setbacks.end].iter().any(|v| !v.is_finite() || *v < 0.0 || *v > LIMIT)
            || (self.version == 1 && !self.setbacks.is_zero())
        {
            return Err(invalid("invalid member end setbacks"));
        }
        match self.shape {
            Shape::Board { length, .. } if self.setbacks.start + self.setbacks.end >= length - 1e-8 => {
                Err(invalid("setbacks must leave a positive physical board length"))
            }
            Shape::Post { .. } | Shape::Pile { .. } if !self.setbacks.is_zero() => Err(invalid("end setbacks apply only to lumber")),
            _ => Ok(()),
        }
    }

    pub fn cut_length(&self) -> Option<f64> {
        match self.shape {
            Shape::Board { length, .. } => Some(length - self.setbacks.start - self.setbacks.end),
            _ => None,
        }
    }
}

pub fn tagged(e: &Entity) -> bool {
    e.common.xdata.iter().any(|t| t.code == 1001 && t.value == APP)
}

pub fn record(e: &Entity) -> Result<MemberRecord> {
    let start = e.common.xdata.iter().position(|t| t.code == 1001 && t.value == APP).ok_or_else(|| invalid("not a Contractor member"))?;
    let mut json = String::new();
    for t in e.common.xdata.iter().skip(start + 1).take_while(|t| t.code != 1001) {
        if t.code == 1000 {
            if t.value.len() > 255 || json.len().saturating_add(t.value.len()) > MAX_METADATA {
                return Err(invalid("Contractor metadata is too large"));
            }
            json.push_str(&t.value);
        }
    }
    let r: MemberRecord = serde_json::from_str(&json).map_err(|_| invalid("invalid Contractor metadata; kept as ordinary CAD geometry"))?;
    r.validate()?;
    Ok(r)
}

fn detach(common: &mut Common) {
    let mut ours = false;
    common.xdata.retain(|t| {
        if t.code == 1001 {
            ours = t.value == APP;
        }
        !ours
    });
}

fn attach(common: &mut Common, r: &MemberRecord) -> Result<()> {
    r.validate()?;
    let text = serde_json::to_string(r).map_err(|e| invalid(e.to_string()))?;
    if text.len() > MAX_METADATA {
        return Err(invalid("Contractor metadata is too large"));
    }
    detach(common);
    common.xdata.push(RawTag { code: 1001, value: APP.into() });
    let mut rest = text.as_str();
    while !rest.is_empty() {
        let mut end = rest.len().min(255);
        while !rest.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        let part = rest.get(..end).ok_or_else(|| invalid("invalid metadata boundary"))?;
        common.xdata.push(RawTag { code: 1000, value: part.into() });
        rest = rest.get(end..).ok_or_else(|| invalid("invalid metadata boundary"))?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Representation {
    #[default]
    Plan,
    Engineering,
    Model3d,
}

pub fn representation(d: &Drawing) -> Representation {
    if d.layer(MESH).is_some_and(|l| l.on) {
        Representation::Model3d
    } else if d.layer(AXIS).is_some_and(|l| l.on) {
        Representation::Engineering
    } else {
        Representation::Plan
    }
}

pub fn set_representation(d: &mut Drawing, rep: Representation) {
    for (name, visible) in [(PLAN, rep == Representation::Plan), (AXIS, rep == Representation::Engineering), (MESH, rep == Representation::Model3d)] {
        d.ensure_layer(name);
        if let Some(l) = d.layer_mut(name) {
            l.on = visible;
            if visible {
                l.frozen = false;
            }
        }
    }
}

fn geometry(record: &MemberRecord) -> Vec<(&'static str, EntityKind)> {
    let shape = &record.shape;
    let mut out = Vec::new();
    let poly = |points: Vec<Vec2>| {
        EntityKind::LwPolyline(LwPolyline {
            vertices: points.into_iter().map(PolyVertex::new).collect(),
            closed: true,
            const_width: 0.0,
            elevation: 0.0,
            plinegen: false,
        })
    };
    let (x0, x1, y0, y1, h) = match *shape {
        Shape::Board { length, width, depth } => (record.setbacks.start, length - record.setbacks.end, -width / 2.0, width / 2.0, depth),
        Shape::Post { width, depth, height } => (-width / 2.0, width / 2.0, -depth / 2.0, depth / 2.0, height),
        Shape::Pile { diameter, height } => {
            let radius = diameter / 2.0;
            let circle = EntityKind::Circle(cadcraft_doc::Circle { center: Vec3::ZERO, radius });
            out.push((PLAN, circle.clone()));
            out.push((AXIS, circle));
            out.push((AXIS, EntityKind::Line(Line { a: Vec3::new(-radius, 0.0, 0.0), b: Vec3::new(radius, 0.0, 0.0) })));
            out.push((AXIS, EntityKind::Line(Line { a: Vec3::new(0.0, -radius, 0.0), b: Vec3::new(0.0, radius, 0.0) })));
            out.push((AXIS, EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(0.0, 0.0, height) })));
            for i in 0..32 {
                let a = i as f64 * std::f64::consts::TAU / 32.0;
                let b = (i + 1) as f64 * std::f64::consts::TAU / 32.0;
                let p = Vec3::new(radius * a.cos(), radius * a.sin(), 0.0);
                let q = Vec3::new(radius * b.cos(), radius * b.sin(), 0.0);
                out.push((MESH, EntityKind::Face3d(Face3d { corners: [p, q, q + Vec3::Z * height, p + Vec3::Z * height], hidden_edges: 0 })));
                out.push((MESH, EntityKind::Face3d(Face3d { corners: [Vec3::ZERO, q, p, p], hidden_edges: 0 })));
                out.push((
                    MESH,
                    EntityKind::Face3d(Face3d {
                        corners: [Vec3::Z * height, p + Vec3::Z * height, q + Vec3::Z * height, q + Vec3::Z * height],
                        hidden_edges: 0,
                    }),
                ));
            }
            return out;
        }
    };
    let footprint = poly(vec![Vec2::new(x0, y0), Vec2::new(x1, y0), Vec2::new(x1, y1), Vec2::new(x0, y1)]);
    out.push((PLAN, footprint.clone()));
    match shape {
        Shape::Board { length, depth, .. } => {
            out.push((AXIS, EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, depth / 2.0), b: Vec3::new(*length, 0.0, depth / 2.0) })))
        }
        _ => {
            out.push((AXIS, footprint));
            out.push((AXIS, EntityKind::Line(Line { a: Vec3::new(x0, 0.0, 0.0), b: Vec3::new(x1, 0.0, 0.0) })));
            out.push((AXIS, EntityKind::Line(Line { a: Vec3::new(0.0, y0, 0.0), b: Vec3::new(0.0, y1, 0.0) })));
            out.push((AXIS, EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::Z * h })));
        }
    }
    let v = [
        Vec3::new(x0, y0, 0.0),
        Vec3::new(x1, y0, 0.0),
        Vec3::new(x1, y1, 0.0),
        Vec3::new(x0, y1, 0.0),
        Vec3::new(x0, y0, h),
        Vec3::new(x1, y0, h),
        Vec3::new(x1, y1, h),
        Vec3::new(x0, y1, h),
    ];
    for indices in [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]] {
        let corners = indices.map(|i| v.get(i).copied().unwrap_or(Vec3::ZERO));
        out.push((MESH, EntityKind::Face3d(Face3d { corners, hidden_edges: 0 })));
    }
    out
}

fn definition(d: &mut Drawing, name: &str, r: &MemberRecord) -> Block {
    let mut block = Block::new(name);
    for (layer, kind) in geometry(r) {
        block.entities.push(Entity {
            handle: d.new_handle(),
            common: Common { layer: layer.into(), color: Color::ByBlock, ..Default::default() },
            kind,
        });
    }
    block
}

fn private_name(d: &Drawing, base: String) -> Result<String> {
    if d.block(&base).is_none() {
        return Ok(base);
    }
    for n in 1..=1024 {
        let name = format!("{base}_{n}");
        if d.block(&name).is_none() {
            return Ok(name);
        }
    }
    Err(invalid("generated block namespace is exhausted"))
}

pub fn add(d: &mut Drawing, r: MemberRecord, at: Vec3, rotation: f64, mut common: Common) -> Result<Handle> {
    r.validate()?;
    validate_pose(at, rotation)?;
    handle_budget(d, &r)?;
    let pose = Insert {
        block: String::new(),
        insert: at,
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation,
        attribs: Vec::new(),
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    };
    validate_geometry(&Member { handle: Handle(0), record: r.clone(), insert: pose })?;
    if d.model.len() >= MAX_MEMBERS {
        return Err(invalid("Contractor member limit reached"));
    }
    attach(&mut common, &r)?;
    let name = private_name(d, format!("CC_MEMBER_{}", Handle(d.handseed).hex()))?;
    if d.layer(PLAN).is_none() {
        set_representation(d, Representation::Plan);
    }
    let h = d.new_handle();
    let b = definition(d, &name, &r);
    d.blocks.insert(name.clone(), std::sync::Arc::new(b));
    d.ensure_layer(&common.layer);
    d.model.push(Entity {
        handle: h,
        common,
        kind: EntityKind::Insert(Insert {
            block: name,
            insert: at,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
        }),
    });
    Ok(h)
}

fn validate_pose(p: Vec3, angle: f64) -> Result<()> {
    if [p.x, p.y, p.z, angle].iter().any(|v| !v.is_finite() || v.abs() > LIMIT) {
        return Err(invalid("member position and angle must be finite and within bounds"));
    }
    Ok(())
}

fn handle_budget(d: &Drawing, r: &MemberRecord) -> Result<()> {
    let count = geometry(r).len().checked_add(1).and_then(|n| u64::try_from(n).ok()).ok_or_else(|| invalid("member handle budget overflow"))?;
    if d.handseed == 0 || d.handseed.checked_add(count).is_none() {
        return Err(invalid("drawing handle space is exhausted"));
    }
    Ok(())
}

fn nearly(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() <= 1e-8 * a.abs().max(b.abs()).max(1.0),
            _ => a == b,
        },
        (Value::Array(a), Value::Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| nearly(a, b)),
        (Value::Object(a), Value::Object(b)) => a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| nearly(a, b))),
        _ => a == b,
    }
}

#[derive(Clone, Debug)]
pub struct Member {
    pub handle: Handle,
    pub record: MemberRecord,
    pub insert: Insert,
}
impl Member {
    pub fn world(&self, p: Vec3) -> Vec3 {
        let xy = Vec2::new(p.x * self.insert.scale.x, p.y * self.insert.scale.y).rotate(self.insert.rotation) + self.insert.insert.xy();
        xy.to3(self.insert.insert.z + p.z * self.insert.scale.z)
    }
    pub fn bottom(&self) -> f64 {
        self.insert.insert.z.min(self.insert.insert.z + self.record.shape.height() * self.insert.scale.z)
    }
    pub fn top(&self) -> f64 {
        self.insert.insert.z.max(self.insert.insert.z + self.record.shape.height() * self.insert.scale.z)
    }
    pub fn axis(&self) -> (Vec3, Vec3) {
        match self.record.shape {
            Shape::Board { length, depth, .. } => (self.world(Vec3::new(0.0, 0.0, depth / 2.0)), self.world(Vec3::new(length, 0.0, depth / 2.0))),
            _ => (self.world(Vec3::ZERO), self.world(Vec3::Z * self.record.shape.height())),
        }
    }
    pub fn faces(&self) -> Vec<[Vec3; 4]> {
        geometry(&self.record)
            .into_iter()
            .filter_map(|(_, k)| if let EntityKind::Face3d(f) = k { Some(f.corners.map(|p| self.world(p))) } else { None })
            .collect()
    }
}

pub fn resolve(d: &Drawing, h: Handle) -> Result<Member> {
    let e = d.entity(h).ok_or_else(|| invalid("no such member"))?;
    let record = record(e)?;
    let EntityKind::Insert(i) = &e.kind else {
        return Err(invalid("Contractor metadata requires a block reference"));
    };
    if e.common.extrusion != Vec3::Z {
        return Err(invalid("sloped framing is not supported in v1; kept as ordinary CAD geometry"));
    }
    validate_pose(i.insert, i.rotation)?;
    if [i.scale.x, i.scale.y, i.scale.z].iter().any(|v| !v.is_finite() || v.abs() < 1e-8 || v.abs() > 1e6) || i.rows != 1 || i.cols != 1 {
        return Err(invalid("unsupported Contractor block transform"));
    }
    let b = d.block(&i.block).ok_or_else(|| invalid("member block is missing"))?;
    let expected = geometry(&record);
    if matches!(record.shape, Shape::Pile { .. }) && (i.scale.x.abs() - i.scale.y.abs()).abs() > 1e-8 {
        return Err(invalid("non-circular piling transform; kept as ordinary CAD geometry"));
    }
    let catalog_name = record.profile.to_ascii_lowercase().replace('×', "x");
    if record.material == "wood"
        && LUMBER.iter().any(|(name, _, _)| *name == catalog_name)
        && let Shape::Board { width, depth, .. } = record.shape
    {
        let (w, h) = lumber(&catalog_name, unit_mm(d)?, false)?;
        let (actual_w, actual_h) = (width * i.scale.y.abs(), depth * i.scale.z.abs());
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-8 * b.abs().max(1.0);
        if !(close(actual_w, w) && close(actual_h, h) || close(actual_w, h) && close(actual_h, w)) {
            return Err(invalid("stock dimensions were changed outside Contractor; kept as ordinary CAD geometry"));
        }
    }
    if b.base != Vec3::ZERO || b.entities.len() != expected.len() {
        return Err(invalid("member block was modified; kept as ordinary CAD geometry"));
    }
    for (e, (layer, kind)) in b.entities.iter().zip(expected) {
        if std::mem::discriminant(&e.kind) != std::mem::discriminant(&kind)
            || matches!((&e.kind,&kind),(EntityKind::LwPolyline(a),EntityKind::LwPolyline(b)) if a.vertices.len()!=b.vertices.len())
        {
            return Err(invalid("member block was modified; kept as ordinary CAD geometry"));
        }
        let a = serde_json::to_value(&e.kind).map_err(|e| invalid(e.to_string()))?;
        let b = serde_json::to_value(&kind).map_err(|e| invalid(e.to_string()))?;
        if e.common.layer != layer || !e.common.visible || e.common.extrusion != Vec3::Z || !nearly(&a, &b) {
            return Err(invalid("member block was modified; kept as ordinary CAD geometry"));
        }
    }
    let m = Member { handle: h, record, insert: i.clone() };
    validate_geometry(&m)?;
    Ok(m)
}

fn validate_geometry(m: &Member) -> Result<()> {
    let (a, b) = m.axis();
    let axes = [a, b];
    if m.faces().iter().flatten().chain(axes.iter()).any(|p| [p.x, p.y, p.z].iter().any(|x| !x.is_finite() || x.abs() > LIMIT)) {
        return Err(invalid("member geometry exceeds coordinate bounds"));
    }
    Ok(())
}

pub fn members(d: &Drawing) -> Vec<Member> {
    d.model.iter().filter(|e| tagged(e)).take(MAX_MEMBERS).filter_map(|e| resolve(d, e.handle).ok()).collect()
}

/// Ordinary CAD pieces in world coordinates, including the INSERT's elevation and Z scale.
pub fn explode(d: &Drawing, h: Handle) -> Result<Vec<Entity>> {
    let m = resolve(d, h)?;
    let mut common = d.entity(h).ok_or_else(|| invalid("no such member"))?.common.clone();
    detach(&mut common);
    geometry(&m.record)
        .into_iter()
        .map(|(layer, mut kind)| {
            match &mut kind {
                EntityKind::Line(l) => {
                    l.a = m.world(l.a);
                    l.b = m.world(l.b);
                }
                EntityKind::Circle(c) => {
                    c.center = m.world(c.center);
                    c.radius *= m.insert.scale.x.abs();
                }
                EntityKind::LwPolyline(p) => {
                    p.elevation = m.world(Vec3::ZERO).z;
                    for v in &mut p.vertices {
                        v.p = m.world(v.p.to3(0.0)).xy();
                    }
                }
                EntityKind::Face3d(f) => {
                    f.corners = f.corners.map(|p| m.world(p));
                }
                _ => return Err(invalid("unsupported generated geometry")),
            }
            let mut common = common.clone();
            common.layer = layer.into();
            Ok(Entity { handle: Handle(0), common, kind })
        })
        .collect()
}

/// Atomic, copy-on-write edit: replacement block is private to this member.
pub fn update(d: &mut Drawing, h: Handle, r: MemberRecord, at: Vec3, rotation: f64) -> Result<()> {
    resolve(d, h)?;
    r.validate()?;
    validate_pose(at, rotation)?;
    handle_budget(d, &r)?;
    let mut next = d.clone();
    let nonce = next.new_handle();
    let name = private_name(&next, format!("CC_MEMBER_{}_{}", h.hex(), nonce.hex()))?;
    let b = definition(&mut next, &name, &r);
    next.blocks.insert(name.clone(), std::sync::Arc::new(b));
    let mut common = next.entity(h).ok_or_else(|| invalid("no such member"))?.common.clone();
    attach(&mut common, &r)?;
    next.modify_entity(h, |e| {
        e.common = common;
        if let EntityKind::Insert(i) = &mut e.kind {
            i.block = name;
            i.insert = at;
            i.rotation = rotation;
            i.scale = Vec3::new(1.0, 1.0, 1.0);
        }
    })?;
    resolve(&next, h)?;
    *d = next;
    Ok(())
}
