//! Physical square cuts and bounded interference checks. Engineering endpoints stay explicit.
use crate::{Drawing, Member, MemberRecord, Result, Shape, invalid, members};
use cadcraft_doc::Handle;
use cadcraft_geom::{Vec2, Vec3};

fn rectangle(m: &Member) -> Vec<Vec2> {
    let (x0, x1, y0, y1) = match m.record.shape {
        Shape::Board { length, width, .. } => (m.record.setbacks.start, length - m.record.setbacks.end, -width / 2.0, width / 2.0),
        Shape::Post { width, depth, .. } => (-width / 2.0, width / 2.0, -depth / 2.0, depth / 2.0),
        Shape::Pile { diameter, .. } => (-diameter / 2.0, diameter / 2.0, -diameter / 2.0, diameter / 2.0),
    };
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].into_iter().map(|(x, y)| m.world(Vec3::new(x, y, 0.0)).xy()).collect()
}

fn circle(m: &Member) -> Option<(Vec2, f64)> {
    if let Shape::Pile { diameter, .. } = m.record.shape { Some((m.insert.insert.xy(), diameter * m.insert.scale.x.abs() / 2.0)) } else { None }
}

fn interval(p: &[Vec2], axis: Vec2) -> (f64, f64) {
    p.iter().map(|p| p.dot(axis)).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)))
}

fn circle_rectangle(center: Vec2, radius: f64, rect: &[Vec2]) -> bool {
    let (Some(a), Some(b), Some(d)) = (rect.first(), rect.get(1), rect.get(3)) else { return false };
    let (u, v) = (*b - *a, *d - *a);
    let (w, h) = (u.len(), v.len());
    if w <= 1e-8 || h <= 1e-8 {
        return false;
    }
    let (u, v) = (u / w, v / h);
    let q = center - *a;
    let closest = *a + u * q.dot(u).clamp(0.0, w) + v * q.dot(v).clamp(0.0, h);
    center.dist(closest) < radius - 1e-8
}

/// Touching faces and stacked members are valid. Only positive volume overlap is reported.
pub fn intersects(a: &Member, b: &Member) -> bool {
    if a.top().min(b.top()) - a.bottom().max(b.bottom()) <= 1e-8 {
        return false;
    }
    match (circle(a), circle(b)) {
        (Some((a, r)), Some((b, s))) => a.dist(b) < r + s - 1e-8,
        (Some((c, r)), None) => circle_rectangle(c, r, &rectangle(b)),
        (None, Some((c, r))) => circle_rectangle(c, r, &rectangle(a)),
        (None, None) => {
            let (a, b) = (rectangle(a), rectangle(b));
            for poly in [&a, &b] {
                for pair in poly.windows(2).take(2) {
                    let (Some(p), Some(q)) = (pair.first(), pair.get(1)) else { return false };
                    let axis = (*q - *p).normalized().perp();
                    let (amin, amax) = interval(&a, axis);
                    let (bmin, bmax) = interval(&b, axis);
                    if amax.min(bmax) - amin.max(bmin) <= 1e-8 {
                        return false;
                    }
                }
            }
            true
        }
    }
}

/// Sweep broad phase and explicit budgets avoid quadratic work on hostile drawings.
pub fn collisions(d: &Drawing) -> Result<Vec<(Handle, Handle)>> {
    let mut items = members(d)
        .into_iter()
        .map(|m| {
            let p = rectangle(&m);
            let x = interval(&p, Vec2::new(1.0, 0.0));
            (m, x)
        })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| a.1.0.total_cmp(&b.1.0));
    let mut out = Vec::new();
    let mut work = 0usize;
    for (i, (a, (_, max))) in items.iter().enumerate() {
        for (b, (min, _)) in items.iter().skip(i + 1) {
            if min >= max {
                break;
            }
            work += 1;
            if work > 1_000_000 {
                return Err(invalid("collision check exceeds pair budget; check a smaller assembly"));
            }
            if intersects(a, b) {
                if out.len() >= 10_000 {
                    return Err(invalid("collision report exceeds 10000 overlaps"));
                }
                out.push((a.handle, b.handle));
            }
        }
    }
    Ok(out)
}

pub struct JointEdit {
    pub record: MemberRecord,
    pub at: Vec3,
    pub rotation: f64,
}

fn clip(poly: Vec<Vec2>, limit: f64, sign: f64) -> Vec<Vec2> {
    let mut out = Vec::new();
    let Some(mut prev) = poly.last().copied() else { return out };
    for p in &poly {
        let (a, b) = (prev.y * sign <= limit, p.y * sign <= limit);
        if a != b {
            let t = (limit * sign - prev.y) / (p.y - prev.y);
            out.push(prev + (*p - prev) * t);
        }
        if b {
            out.push(*p);
        }
        prev = *p;
    }
    out
}

/// Connect the selected engineering endpoint to the reference axis and set a square physical cut.
/// This samples the reference; subsequent edits do not create a constraint or move other members.
pub fn butt(source: &Member, reference: &Member, end: &str, gap: f64) -> Result<JointEdit> {
    if source.handle == reference.handle || !gap.is_finite() || !(0.0..=1e9).contains(&gap) {
        return Err(invalid("choose another member and a finite nonnegative gap"));
    }
    let Shape::Board { width, depth, .. } = source.record.shape else {
        return Err(invalid("butt joints apply to lumber"));
    };
    let (a, b) = source.axis();
    let (a, b) = (a.xy(), b.xy());
    let joint = if matches!(reference.record.shape, Shape::Board { .. }) {
        let (c, d) = reference.axis();
        let (c, d) = (c.xy(), d.xy());
        let (u, v) = (b - a, d - c);
        let den = u.cross(v);
        if den.abs() > 1e-10 * u.len() * v.len() {
            let t = (c - a).cross(v) / den;
            let s = (c - a).cross(u) / den;
            if !(-1e-8..=1.0 + 1e-8).contains(&s) {
                return Err(invalid("engineering intersection falls beyond the reference member"));
            }
            a + u * t
        } else {
            if (c - a).cross(u).abs() > 1e-8 * u.len() {
                return Err(invalid("parallel offset axes do not form a butt joint"));
            }
            let endpoint = if end == "start" {
                a
            } else if end == "end" {
                b
            } else if a.dist(c).min(a.dist(d)) < b.dist(c).min(b.dist(d)) {
                a
            } else {
                b
            };
            if endpoint.dist(c) < endpoint.dist(d) { c } else { d }
        }
    } else {
        reference.insert.insert.xy()
    };
    let start = match end {
        "start" => true,
        "end" => false,
        "auto" => joint.dist(a) < joint.dist(b),
        _ => return Err(invalid("end must be start, end, or auto")),
    };
    let (a, b) = if start { (joint, b) } else { (a, joint) };
    if (b - a).dot(Vec2::from_angle(source.insert.rotation) * source.insert.scale.x.signum()) <= 1e-8 {
        return Err(invalid("joint would reverse or erase the board"));
    }
    let mut record = source.record.clone();
    record.version = 2;
    record.shape = Shape::Board { length: a.dist(b), width: width * source.insert.scale.y.abs(), depth: depth * source.insert.scale.z.abs() };
    record.setbacks.start *= source.insert.scale.x.abs();
    record.setbacks.end *= source.insert.scale.x.abs();
    let direction = if start { (b - a).normalized() } else { (a - b).normalized() };
    let mut setback = 0.0;
    if source.top().min(reference.top()) - source.bottom().max(reference.bottom()) > 1e-8 {
        setback = if let Some((_, radius)) = circle(reference) {
            radius
        } else {
            let poly = rectangle(reference)
                .into_iter()
                .map(|p| {
                    let p = p - joint;
                    Vec2::new(p.dot(direction), p.dot(direction.perp()))
                })
                .collect();
            let half = width * source.insert.scale.y.abs() / 2.0;
            let poly = clip(clip(poly, half, 1.0), half, -1.0);
            poly.iter().map(|p| p.x).fold(0.0, f64::max)
        };
    }
    if start {
        record.setbacks.start = setback + gap;
    } else {
        record.setbacks.end = setback + gap;
    }
    record.validate()?;
    Ok(JointEdit { record, at: a.to3(source.bottom()), rotation: a.angle_to(b) })
}
