//! Orthographic camera and bounded reference z-buffer. Independent of any UI/GPU toolkit.
use crate::{Error, MAX_MEMBERS, Result, resolve, tagged};
use cadcraft_doc::{Drawing, Handle};
use cadcraft_geom::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub center: Vec3,
    pub yaw: f64,
    pub pitch: f64,
    pub height: f64,
}
impl Default for Camera {
    fn default() -> Self {
        Self { center: Vec3::ZERO, yaw: -0.7, pitch: 0.65, height: 100.0 }
    }
}
impl Camera {
    pub fn validate(&self) -> Result<()> {
        if !self.center.is_finite()
            || [self.center.x, self.center.y, self.center.z].iter().any(|v| v.abs() > 1e12)
            || !self.yaw.is_finite()
            || !self.pitch.is_finite()
            || !self.height.is_finite()
            || self.height < 1e-6
            || self.height > 1e12
        {
            return Err(Error::Invalid("invalid 3D camera".into()));
        }
        Ok(())
    }
    pub fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let right = Vec3::new(self.yaw.cos(), self.yaw.sin(), 0.0);
        let forward = Vec3::new(self.yaw.sin() * self.pitch.cos(), -self.yaw.cos() * self.pitch.cos(), self.pitch.sin());
        (right, forward.cross(right), forward)
    }
    /// X/Y in pixels, Z toward the viewer (larger is nearer).
    pub fn project(&self, p: Vec3, width: usize, height: usize) -> Vec3 {
        let (right, up, forward) = self.basis();
        let p = p - self.center;
        let scale = height as f64 / self.height;
        Vec3::new(width as f64 * 0.5 + p.dot(right) * scale, height as f64 * 0.5 - p.dot(up) * scale, p.dot(forward))
    }
    pub fn fit(&mut self, scene: &Scene, aspect: f64) {
        if scene.triangles.is_empty() {
            return;
        }
        let mut lo = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut hi = -lo;
        for p in scene.triangles.iter().flat_map(|t| t.points) {
            lo.x = lo.x.min(p.x);
            lo.y = lo.y.min(p.y);
            lo.z = lo.z.min(p.z);
            hi.x = hi.x.max(p.x);
            hi.y = hi.y.max(p.y);
            hi.z = hi.z.max(p.z);
        }
        self.center = (lo + hi) * 0.5;
        let mut x: f64 = 0.0;
        let mut y: f64 = 0.0;
        let (right, up, _) = self.basis();
        for p in scene.triangles.iter().flat_map(|t| t.points) {
            let p = p - self.center;
            x = x.max(p.dot(right).abs());
            y = y.max(p.dot(up).abs());
        }
        self.height = (2.4 * y.max(x / aspect.max(0.1))).max(1.0);
    }
}

#[derive(Clone, Debug)]
pub struct Triangle {
    pub points: [Vec3; 3],
    pub handle: Handle,
    pub color: [u8; 3],
}
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub triangles: Vec<Triangle>,
}
impl Scene {
    pub fn build(d: &Drawing) -> Result<Self> {
        let mut triangles = Vec::new();
        for e in d.model.iter().filter(|e| tagged(e)).take(MAX_MEMBERS) {
            if !d.is_visible(e) {
                continue;
            }
            let Ok(m) = resolve(d, e.handle) else {
                continue;
            };
            let base = match m.record.role.as_str() {
                "joist" => [205, 165, 105],
                "beam" => [164, 113, 65],
                "piling" => [127, 103, 78],
                _ => [189, 148, 93],
            };
            for face in m.faces() {
                if triangles.len() >= 500_000 {
                    return Err(Error::Invalid("3D scene exceeds 500000 triangles; isolate fewer members".into()));
                }
                let [a, b, c, e] = face;
                let n = (b - a).cross(c - a).normalized();
                let light = 0.5 + 0.5 * n.dot(Vec3::new(0.3, -0.4, 0.85).normalized()).abs();
                let color = base.map(|v| (f64::from(v) * light).clamp(0.0, 255.0) as u8);
                triangles.push(Triangle { points: [a, b, c], handle: m.handle, color });
                triangles.push(Triangle { points: [a, c, e], handle: m.handle, color });
            }
        }
        Ok(Self { triangles })
    }
    /// Pick the visible triangle under a screen point without a GPU readback.
    pub fn pick(&self, camera: &Camera, width: usize, height: usize, x: f64, y: f64) -> Option<Handle> {
        if camera.validate().is_err() || !x.is_finite() || !y.is_finite() {
            return None;
        }
        self.triangles
            .iter()
            .filter_map(|t| {
                let p = t.points.map(|p| camera.project(p, width, height));
                let z = depth(p, x, y)?;
                Some((z, t.handle))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, h)| h)
    }
}

fn depth(p: [Vec3; 3], x: f64, y: f64) -> Option<f64> {
    let [a, b, c] = p;
    let den = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
    if den.abs() < 1e-10 || !den.is_finite() {
        return None;
    }
    let u = ((b.y - c.y) * (x - c.x) + (c.x - b.x) * (y - c.y)) / den;
    let v = ((c.y - a.y) * (x - c.x) + (a.x - c.x) * (y - c.y)) / den;
    let w = 1.0 - u - v;
    (u >= -1e-9 && v >= -1e-9 && w >= -1e-9).then_some(u * a.z + v * b.z + w * c.z)
}

pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub handles: Vec<Handle>,
}
pub fn raster(scene: &Scene, camera: &Camera, width: usize, height: usize, selected: &[Handle]) -> Result<Frame> {
    camera.validate()?;
    let count = width
        .checked_mul(height)
        .filter(|n| *n <= 4_194_304 && width > 0 && height > 0 && width <= 4096 && height <= 4096)
        .ok_or_else(|| Error::Invalid("3D frame exceeds pixel budget".into()))?;
    let bytes = count.checked_mul(4).ok_or_else(|| Error::Invalid("3D image size overflow".into()))?;
    let mut rgba = vec![0; bytes];
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.copy_from_slice(&[0, 0, 0, 0]);
    }
    let mut zbuffer = vec![f64::NEG_INFINITY; count];
    let mut handles = vec![Handle(0); count];
    // Bound work as well as allocations; the GPU path handles larger scenes interactively.
    let mut budget = 80_000_000usize;
    let selected: std::collections::HashSet<_> = selected.iter().copied().collect();
    for t in &scene.triangles {
        let pts = t.points.map(|p| camera.project(p, width, height));
        if pts.iter().any(|p| !p.is_finite()) {
            continue;
        }
        let minx = pts.iter().map(|p| p.x).fold(f64::INFINITY, f64::min).floor().clamp(0.0, width as f64) as usize;
        let maxx = pts.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max).ceil().clamp(0.0, width as f64) as usize;
        let miny = pts.iter().map(|p| p.y).fold(f64::INFINITY, f64::min).floor().clamp(0.0, height as f64) as usize;
        let maxy = pts.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max).ceil().clamp(0.0, height as f64) as usize;
        let work = maxx.saturating_sub(minx).saturating_mul(maxy.saturating_sub(miny));
        budget = budget
            .checked_sub(work)
            .ok_or_else(|| Error::Invalid("3D software rendering budget exceeded; use a smaller image or fewer members".into()))?;
        let color = if selected.contains(&t.handle) { [95, 191, 255] } else { t.color };
        for y in miny..maxy {
            for x in minx..maxx {
                let Some(z) = depth(pts, x as f64 + 0.5, y as f64 + 0.5) else { continue };
                let idx = y * width + x;
                let Some(old) = zbuffer.get_mut(idx) else { continue };
                if z > *old {
                    *old = z;
                    if let Some(h) = handles.get_mut(idx) {
                        *h = t.handle;
                    }
                    if let Some(pixel) = rgba.get_mut(idx * 4..idx * 4 + 4) {
                        pixel.copy_from_slice(&[color[0], color[1], color[2], 255]);
                    }
                }
            }
        }
    }
    Ok(Frame { width, height, rgba, handles })
}
