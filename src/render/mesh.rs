//! CPU-side mesh building: vertex formats plus procedural primitives.

use glam::{Mat3, Mat4, Vec3, Vec4};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    /// rgb albedo, a = material id (see shader).
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Instance {
    pub model: [[f32; 4]; 4],
    /// rgb multiplies albedo, a = emissive strength (0 = fully lit surface).
    pub tint: [f32; 4],
}

impl Instance {
    pub fn new(model: Mat4, tint: Vec4) -> Instance {
        Instance {
            model: model.to_cols_array_2d(),
            tint: tint.to_array(),
        }
    }
}

/// Handle to a mesh drawn with instancing. The first few are built-in unit
/// primitives; more are registered at startup with `Renderer::add_mesh`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeshId(pub u32);

#[allow(dead_code)]
impl MeshId {
    /// Unit cube centered at the origin (extent -0.5..0.5).
    pub const CUBE: MeshId = MeshId(0);
    /// Radius 0.5, height 1, along +Y, centered.
    pub const CYLINDER: MeshId = MeshId(1);
    /// Radius 0.5.
    pub const SPHERE: MeshId = MeshId(2);
    /// Octahedron gem, radius 0.5, height 1.
    pub const GEM: MeshId = MeshId(3);
    /// Parachute dome: rim radius 0.5 at y=0, apex at y=0.5, striped gores.
    pub const CANOPY: MeshId = MeshId(4);
}

pub const MESH_COUNT: usize = 5;

/// Somewhere meshes are registered for instanced drawing: the renderer, or
/// (in tests) a plain list kept on the CPU.
pub trait MeshStore {
    fn add_mesh(&mut self, mesh: &MeshData) -> MeshId;
}

/// Meshes kept on the CPU, indexed like the renderer's: the built-in
/// primitives first, then each one added.
#[cfg(test)]
pub struct CpuMeshes(pub Vec<MeshData>);

#[cfg(test)]
impl CpuMeshes {
    pub fn new() -> CpuMeshes {
        CpuMeshes(MeshData::primitives().into())
    }
}

#[cfg(test)]
impl MeshStore for CpuMeshes {
    fn add_mesh(&mut self, mesh: &MeshData) -> MeshId {
        self.0.push(mesh.clone());
        MeshId(self.0.len() as u32 - 1)
    }
}

#[derive(Default, Clone)]
pub struct MeshData {
    pub verts: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn primitives() -> [MeshData; MESH_COUNT] {
        [
            MeshData::cube(),
            MeshData::cylinder(20),
            MeshData::sphere(16, 10),
            MeshData::gem(),
            MeshData::canopy(12, 5),
        ]
    }

    fn vert(&mut self, p: Vec3, n: Vec3, c: Vec4) -> u32 {
        self.verts.push(Vertex {
            pos: p.to_array(),
            normal: n.to_array(),
            color: c.to_array(),
        });
        self.verts.len() as u32 - 1
    }

    pub fn quad(&mut self, p: [Vec3; 4], n: Vec3, c: Vec4) {
        let i: Vec<u32> = p.iter().map(|&p| self.vert(p, n, c)).collect();
        self.indices
            .extend_from_slice(&[i[0], i[1], i[2], i[0], i[2], i[3]]);
    }

    pub fn tri(&mut self, p: [Vec3; 3], n: Vec3, c: Vec4) {
        for q in p {
            let i = self.vert(q, n, c);
            self.indices.push(i);
        }
    }

    /// Axis-aligned box with separate colors (and material ids) for sides and top.
    pub fn add_box(&mut self, min: Vec3, max: Vec3, side: Vec4, top: Vec4) {
        let (a, b) = (min, max);
        let c = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        self.quad(
            [
                c(b.x, a.y, a.z),
                c(b.x, b.y, a.z),
                c(b.x, b.y, b.z),
                c(b.x, a.y, b.z),
            ],
            Vec3::X,
            side,
        );
        self.quad(
            [
                c(a.x, a.y, b.z),
                c(a.x, b.y, b.z),
                c(a.x, b.y, a.z),
                c(a.x, a.y, a.z),
            ],
            Vec3::NEG_X,
            side,
        );
        self.quad(
            [
                c(a.x, a.y, b.z),
                c(b.x, a.y, b.z),
                c(b.x, b.y, b.z),
                c(a.x, b.y, b.z),
            ],
            Vec3::Z,
            side,
        );
        self.quad(
            [
                c(b.x, a.y, a.z),
                c(a.x, a.y, a.z),
                c(a.x, b.y, a.z),
                c(b.x, b.y, a.z),
            ],
            Vec3::NEG_Z,
            side,
        );
        self.quad(
            [
                c(a.x, b.y, a.z),
                c(a.x, b.y, b.z),
                c(b.x, b.y, b.z),
                c(b.x, b.y, a.z),
            ],
            Vec3::Y,
            top,
        );
        self.quad(
            [
                c(a.x, a.y, a.z),
                c(b.x, a.y, a.z),
                c(b.x, a.y, b.z),
                c(a.x, a.y, b.z),
            ],
            Vec3::NEG_Y,
            side,
        );
    }

    /// Append another mesh transformed by `m`, recoloring it with `color` (rgb + material id).
    pub fn append(&mut self, other: &MeshData, m: Mat4, color: Vec4) {
        let base = self.verts.len() as u32;
        let nm = Mat3::from_mat4(m).inverse().transpose();
        for v in &other.verts {
            let p = m.transform_point3(Vec3::from(v.pos));
            let n = (nm * Vec3::from(v.normal)).normalize_or_zero();
            self.verts.push(Vertex {
                pos: p.to_array(),
                normal: n.to_array(),
                color: color.to_array(),
            });
        }
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }

    /// Flat-shaded triangle whose normal faces away from `inside`.
    pub fn facet(&mut self, p: [Vec3; 3], inside: Vec3, c: Vec4) {
        let mut n = (p[1] - p[0]).cross(p[2] - p[0]);
        if n.length_squared() < 1e-12 {
            return;
        }
        n = n.normalize();
        let centre = (p[0] + p[1] + p[2]) / 3.0;
        if n.dot(centre - inside) < 0.0 {
            n = -n;
        }
        self.tri(p, n, c);
    }

    pub fn cube() -> MeshData {
        let mut m = MeshData::default();
        let w = Vec4::new(1.0, 1.0, 1.0, 0.0);
        m.add_box(Vec3::splat(-0.5), Vec3::splat(0.5), w, w);
        m
    }

    pub fn cylinder(segs: u32) -> MeshData {
        let mut m = MeshData::default();
        let w = Vec4::new(1.0, 1.0, 1.0, 0.0);
        let ring = |i: u32| {
            let a = i as f32 / segs as f32 * std::f32::consts::TAU;
            Vec3::new(a.cos() * 0.5, 0.0, a.sin() * 0.5)
        };
        for i in 0..segs {
            let (p0, p1) = (ring(i), ring(i + 1));
            let (n0, n1) = (p0.normalize(), p1.normalize());
            let up = Vec3::Y * 0.5;
            let b = m.verts.len() as u32;
            m.vert(p0 - up, n0, w);
            m.vert(p1 - up, n1, w);
            m.vert(p1 + up, n1, w);
            m.vert(p0 + up, n0, w);
            m.indices
                .extend_from_slice(&[b, b + 2, b + 1, b, b + 3, b + 2]);
            m.tri([up, p1 + up, p0 + up], Vec3::Y, w);
            m.tri([-up, p0 - up, p1 - up], Vec3::NEG_Y, w);
        }
        m
    }

    pub fn sphere(segs: u32, rings: u32) -> MeshData {
        let mut m = MeshData::default();
        let w = Vec4::new(1.0, 1.0, 1.0, 0.0);
        for r in 0..=rings {
            let phi = r as f32 / rings as f32 * std::f32::consts::PI;
            for s in 0..=segs {
                let th = s as f32 / segs as f32 * std::f32::consts::TAU;
                let n = Vec3::new(phi.sin() * th.cos(), phi.cos(), phi.sin() * th.sin());
                m.vert(n * 0.5, n, w);
            }
        }
        let row = segs + 1;
        for r in 0..rings {
            for s in 0..segs {
                let (a, b) = (r * row + s, (r + 1) * row + s);
                m.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
            }
        }
        m
    }

    pub fn gem() -> MeshData {
        let mut m = MeshData::default();
        let w = Vec4::new(1.0, 1.0, 1.0, 0.0);
        let (top, bot) = (Vec3::Y * 0.5, Vec3::NEG_Y * 0.5);
        let ring: Vec<Vec3> = (0..6)
            .map(|i| {
                let a = i as f32 / 6.0 * std::f32::consts::TAU;
                Vec3::new(a.cos() * 0.32, 0.08, a.sin() * 0.32)
            })
            .collect();
        for i in 0..6 {
            let (a, b) = (ring[i], ring[(i + 1) % 6]);
            let n_top = (b - a).cross(top - a).normalize();
            m.tri([a, top, b], -n_top, w);
            let n_bot = (bot - a).cross(b - a).normalize();
            m.tri([a, b, bot], -n_bot, w);
        }
        m
    }

    /// Half an ellipsoid, open underneath, in alternating white and orange gores.
    pub fn canopy(gores: u32, rings: u32) -> MeshData {
        let mut m = MeshData::default();
        let colors = [
            Vec4::new(0.92, 0.9, 0.84, 0.0),
            Vec4::new(0.9, 0.42, 0.14, 0.0),
        ];
        let point = |g: u32, r: u32| {
            let th = g as f32 / gores as f32 * std::f32::consts::TAU;
            let phi = r as f32 / rings as f32 * std::f32::consts::FRAC_PI_2;
            let n = Vec3::new(phi.sin() * th.cos(), phi.cos(), phi.sin() * th.sin());
            (n * 0.5, n)
        };
        for g in 0..gores {
            let c = colors[(g % 2) as usize];
            for r in 0..rings {
                let (a, na) = point(g, r);
                let (b, nb) = point(g, r + 1);
                let (cc, nc) = point(g + 1, r + 1);
                let (d, nd) = point(g + 1, r);
                let i = [
                    m.vert(a, na, c),
                    m.vert(b, nb, c),
                    m.vert(cc, nc, c),
                    m.vert(d, nd, c),
                ];
                m.indices
                    .extend_from_slice(&[i[0], i[1], i[2], i[0], i[2], i[3]]);
            }
        }
        m
    }
}
