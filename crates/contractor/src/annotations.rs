//! Derived drawing labels: current elevations, bounded collision avoidance, no source edits.
use crate::{LIMIT, MAX_REPEAT, NOTES, Representation, Result, Shape, invalid, members, set_representation};
use cadcraft_doc::{Common, Drawing, Entity, EntityKind, Handle, RawTag, Space, Text, entity_bounds};
use cadcraft_geom::{Bounds2, Vec2};
use std::collections::BTreeMap;

const NOTE_APP: &str = "CADCRAFT_CONTRACTOR_NOTES";
fn common() -> Common {
    Common {
        layer: NOTES.into(),
        xdata: vec![RawTag { code: 1001, value: NOTE_APP.into() }, RawTag { code: 1000, value: "1".into() }],
        ..Default::default()
    }
}
fn compact(v: f64) -> String {
    if v.abs() < 0.0005 {
        return "0".into();
    }
    format!("{v:.3}").trim_end_matches('0').trim_end_matches('.').into()
}
fn short(s: &str) -> String {
    let mut out = s.chars().take(16).collect::<String>();
    if s.chars().count() > 16 {
        out.push_str("...");
    }
    out
}
fn label(value: String, at: Vec2, height: f64, rotation: f64) -> EntityKind {
    EntityKind::Text(Text {
        insert: at.to3(0.0),
        align_pt: None,
        height,
        value,
        rotation,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: Default::default(),
        valign: Default::default(),
    })
}

struct LabelGrid {
    cell: f64,
    boxes: BTreeMap<(i64, i64), Vec<Bounds2>>,
}
impl LabelGrid {
    fn cells(&self, b: &Bounds2) -> Result<Vec<(i64, i64)>> {
        let key = |v: f64| -> Result<i64> {
            let v = (v / self.cell).floor();
            if !v.is_finite() || v.abs() > 1e18 {
                return Err(invalid("annotation coordinates exceed layout bounds"));
            }
            Ok(v as i64)
        };
        let (x0, x1, y0, y1) = (key(b.min.x)?, key(b.max.x)?, key(b.min.y)?, key(b.max.y)?);
        if x1.checked_sub(x0).is_none_or(|n| n > 64) || y1.checked_sub(y0).is_none_or(|n| n > 64) {
            return Err(invalid("annotation exceeds layout budget"));
        }
        let mut out = Vec::new();
        for x in x0..=x1 {
            for y in y0..=y1 {
                out.push((x, y));
            }
        }
        Ok(out)
    }
    fn vacant(&self, b: &Bounds2) -> Result<bool> {
        Ok(self.cells(b)?.iter().all(|key| self.boxes.get(key).is_none_or(|boxes| boxes.iter().all(|a| !a.intersects(b)))))
    }
    fn insert(&mut self, b: Bounds2) -> Result<()> {
        for key in self.cells(&b)? {
            self.boxes.entry(key).or_default().push(b);
        }
        Ok(())
    }
}

fn padded(d: &Drawing, kind: &EntityKind, height: f64) -> Bounds2 {
    let b = entity_bounds(d, &Entity::new(Handle(0), kind.clone()), 0);
    let margin = Vec2::new(height * 0.5, height * 0.5);
    Bounds2::new(b.min - margin, b.max + margin)
}

fn put(d: &mut Drawing, grid: &mut LabelGrid, kind: EntityKind, height: f64) -> Result<()> {
    let b = padded(d, &kind, height);
    if !grid.vacant(&b)? {
        return Err(invalid("automatic framing labels are too crowded; isolate a smaller drawing"));
    }
    grid.insert(b)?;
    // Native background masks keep structural lines from striking through the labels.
    d.add(&Space::Model, common(), EntityKind::Wipeout(cadcraft_doc::Wipeout { boundary: b.corners().to_vec() }))?;
    d.add(&Space::Model, common(), kind)?;
    Ok(())
}

pub fn export_drawing(d: &Drawing, rep: Representation, height: f64) -> Result<Drawing> {
    if !height.is_finite() || !(1e-8..=LIMIT).contains(&height) {
        return Err(invalid("invalid annotation height"));
    }
    let mut out = d.clone();
    let old = out
        .model
        .iter()
        .filter(|e| {
            e.common.xdata.windows(2).any(|t| {
                t.first().is_some_and(|a| a.code == 1001 && a.value == NOTE_APP) && t.get(1).is_some_and(|a| a.code == 1000 && a.value == "1")
            })
        })
        .map(|e| e.handle)
        .collect::<Vec<_>>();
    for h in old {
        out.remove_entity(h);
    }
    set_representation(&mut out, rep);
    if rep == Representation::Model3d {
        return Ok(out);
    }
    let members = members(d).into_iter().filter(|m| out.entity(m.handle).is_some_and(|e| out.is_visible(e))).collect::<Vec<_>>();
    if members.is_empty() {
        return Ok(out);
    }
    if members.len() > MAX_REPEAT {
        return Err(invalid("automatic labels are limited to 10000 members; isolate a smaller set"));
    }
    let handle_count = members
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(4))
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| invalid("annotation handle budget overflow"))?;
    if out.handseed == 0 || out.handseed.checked_add(handle_count).is_none() {
        return Err(invalid("drawing handle space is exhausted"));
    }
    out.ensure_layer(NOTES);
    if let Some(l) = out.layer_mut(NOTES) {
        l.on = true;
    }
    let bounds = out.extents(&Space::Model);
    if bounds.is_empty() {
        return Ok(out);
    }
    let mut counters: BTreeMap<&str, usize> = BTreeMap::new();
    let mut items = Vec::new();
    for m in members {
        let prefix = match m.record.shape {
            Shape::Post { .. } => "P",
            Shape::Pile { .. } => "PL",
            Shape::Board { .. } => match m.record.role.as_str() {
                "beam" => "B",
                "joist" => "J",
                _ => "M",
            },
        };
        let count = counters.entry(prefix).or_default();
        *count = count.checked_add(1).ok_or_else(|| invalid("member tag overflow"))?;
        let tag = format!("{prefix}{count}");
        let section = if m.record.profile == "custom" {
            match m.record.shape {
                Shape::Board { width, depth, .. } => {
                    format!("{}x{}", compact(width * m.insert.scale.y.abs()), compact(depth * m.insert.scale.z.abs()))
                }
                Shape::Post { width, depth, .. } => {
                    format!("{}x{}", compact(width * m.insert.scale.x.abs()), compact(depth * m.insert.scale.y.abs()))
                }
                Shape::Pile { diameter, .. } => format!("D{}", compact(diameter * m.insert.scale.x.abs())),
            }
        } else {
            short(&m.record.profile)
        };
        let detail =
            format!("{tag}  {} {} {}  B{} T{}", short(&m.record.label), short(&m.record.role), section, compact(m.bottom()), compact(m.top()));
        items.push((m, tag, detail));
    }
    let mut grid = LabelGrid { cell: height * 20.0, boxes: BTreeMap::new() };
    let legend_x = bounds.max.x + height * 9.0;
    put(&mut out, &mut grid, label("Framing members / elevations".into(), Vec2::new(legend_x, bounds.max.y + height * 3.0), height, 0.0), height)?;
    for (i, (_, _, detail)) in items.iter().enumerate() {
        put(&mut out, &mut grid, label(detail.clone(), Vec2::new(legend_x, bounds.max.y - i as f64 * height * 2.4), height, 0.0), height)?;
    }
    let units = match d.header.i64("INSUNITS", 0) {
        1 => "inches",
        2 => "feet",
        4 => "mm",
        5 => "cm",
        6 => "metres",
        _ => "drawing units",
    };
    put(
        &mut out,
        &mut grid,
        label(format!("B = bottom; T = top. Elevations in {units}."), Vec2::new(bounds.min.x, bounds.min.y - height * 5.0), height, 0.0),
        height,
    )?;
    for (m, tag, _) in items {
        let (a, b) = m.axis();
        let mut rotation = if matches!(m.record.shape, Shape::Board { .. }) { a.xy().angle_to(b.xy()) } else { 0.0 };
        let mut origin = if matches!(m.record.shape, Shape::Board { .. }) {
            if m.record.role == "beam" { a.xy() + Vec2::from_angle(rotation) * height * 1.2 } else { a.xy() + (b.xy() - a.xy()) * 0.25 }
        } else {
            a.xy() + Vec2::new(m.record.shape.width() * m.insert.scale.x.abs() / 2.0 + height, 0.0)
        };
        if rotation > std::f64::consts::FRAC_PI_2 {
            rotation -= std::f64::consts::PI;
            origin = b.xy();
        }
        if rotation < -std::f64::consts::FRAC_PI_2 {
            rotation += std::f64::consts::PI;
            origin = b.xy();
        }
        let direction = Vec2::from_angle(rotation);
        let normal = direction.perp();
        let offset = if matches!(m.record.shape, Shape::Board { .. }) {
            m.record.shape.width() * m.insert.scale.y.abs() / 2.0 + height * 1.5
        } else {
            height * 1.5
        };
        let mut chosen = None;
        for attempt in 0..32 {
            let at = origin + normal * offset + direction * (attempt as f64 * height * 4.0);
            let kind = label(tag.clone(), at, height, rotation);
            if grid.vacant(&padded(&out, &kind, height))? {
                chosen = Some(kind);
                break;
            }
        }
        let kind = chosen.ok_or_else(|| invalid("member tags are too crowded; isolate a smaller drawing"))?;
        put(&mut out, &mut grid, kind, height)?;
    }
    Ok(out)
}
