//! GPU setup and frame rendering. Works with a window surface or headless
//! (offscreen texture -> PNG) so frames can be checked without a display.
//!
//! Frame: sun shadow map -> lit HDR scene + sky -> tonemap to output -> 2D overlay.

pub mod lod;
pub mod material;
pub mod mesh;
pub mod surface;
#[cfg(test)]
pub mod zfight;

use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use glam::{Mat4, Vec2, Vec3, Vec4};
use wgpu::util::DeviceExt;
use winit::window::Window;

use glam::camera::rh::proj::directx as proj;
use glam::camera::rh::view::look_at_mat4;
use mesh::{Instance, MeshData, MeshId, Vertex};

const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// The camera's near and far planes, m.
const CAM_NEAR: f32 = 0.2;
/// Waiting longer than this for the window's next image is logged.
const SLOW_ACQUIRE: std::time::Duration = std::time::Duration::from_millis(50);
const CAM_FAR: f32 = 600.0;
/// The camera's depth buffer is reverse Z (see `Camera::view_proj`): nearer
/// is greater, and it clears to the far plane, 0. The sun's shadow map is
/// orthographic, evenly precise either way, and stays 0-near.
const CAM_DEPTH_CLEAR: f32 = 0.0;
const SHADOW_SIZE: u32 = 4096;
const AO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub pos: Vec3,
    pub target: Vec3,
    pub fov_y: f32,
}

impl Camera {
    /// Reverse Z: depth 1 at the near plane, 0 at the far one. With a
    /// float depth buffer that keeps precision even across the whole range
    /// (a standard 0-near mapping spends it all up close), so surfaces a
    /// fraction of a millimetre apart don't z-fight even far off.
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = look_at_mat4(self.pos, self.target, Vec3::Y);
        let proj = proj::perspective(self.fov_y, aspect, CAM_FAR, CAM_NEAR);
        proj * view
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
    /// Where in the thumbnail atlas to sample (0..1), or negative for a flat
    /// colour. For text, where in the font atlas instead.
    pub uv: [f32; 2],
    /// Text only: how far to grow the glyph's outline and how wide its edge
    /// ramp is, both in screen px. A zero ramp means this isn't text.
    pub sdf: [f32; 2],
}

/// UV for a flat-coloured UI vertex (no texture).
pub const NO_UV: [f32; 2] = [-1.0, -1.0];
/// `sdf` for anything that isn't text.
pub const NO_SDF: [f32; 2] = [0.0, 0.0];

/// Sky, sun and haze for a map.
#[derive(Clone, Copy, Debug)]
pub struct Atmosphere {
    /// Direction toward the sun.
    pub sun_dir: Vec3,
    /// Sun color times intensity (HDR).
    pub sun_color: Vec3,
    pub sky_zenith: Vec3,
    pub sky_horizon: Vec3,
    /// Bounce light from the ground (fills downward faces).
    pub ground_color: Vec3,
    /// Exponential haze per meter.
    pub fog_density: f32,
    /// Wind on the ground plane, m/s (drives blowing sand and drifting dust).
    pub wind: Vec2,
    /// 0..1: how much loose sand the wind carries.
    pub blowing_sand: f32,
    /// Color of dust kicked up or blown about.
    pub dust: Vec3,
    /// Color of the dust that settles on everything facing up (and cakes
    /// the foot of plastered walls): desert sand on most maps.
    pub settle: Vec3,
    /// Scales the sky and ground fill that lights faces the sun misses
    /// (1 on daylit maps). A dusk map keeps its dark sky but turns this up
    /// so walls and tanks in shade still read.
    pub ambient: f32,
    /// 0..1: how bright the stars are.
    pub stars: f32,
    /// How big the sun's disc and the glare round it are (1 ordinary).
    pub sun_size: f32,
}

/// Desert sand, settled: `Atmosphere::settle` on most maps.
pub const SAND_SETTLE: Vec3 = Vec3::new(0.74, 0.6, 0.42);

/// Haze this thick (the share of what's behind it still showing) hides a tank.
pub const HIDDEN: f32 = 0.04;

impl Atmosphere {
    /// How bright the light is on a wall in the shade (as the scene's shader
    /// mixes it: half sky, half ground).
    pub fn fill(&self) -> f32 {
        let sky = self.sky_horizon.lerp(self.sky_zenith, 0.35);
        let light = self.ground_color.lerp(sky, 0.5);
        light.dot(Vec3::new(0.2126, 0.7152, 0.0722)) * self.ambient
    }

    /// How far off a tank can be made out through the haze, m (bots see,
    /// guns assist and health bars show no further than this either).
    pub fn sight(&self) -> f32 {
        if self.fog_density > 0.0 {
            -HIDDEN.ln() / self.fog_density
        } else {
            f32::INFINITY
        }
    }
}

impl Default for Atmosphere {
    fn default() -> Atmosphere {
        Atmosphere {
            sun_dir: Vec3::new(0.55, 0.48, 0.4),
            sun_color: Vec3::new(2.1, 1.9, 1.65),
            sky_zenith: Vec3::new(0.22, 0.42, 0.78),
            sky_horizon: Vec3::new(0.62, 0.72, 0.84),
            ground_color: Vec3::new(0.32, 0.30, 0.27),
            fog_density: 0.0022,
            wind: Vec2::new(2.5, 1.0),
            blowing_sand: 0.0,
            dust: Vec3::new(0.52, 0.5, 0.47),
            settle: SAND_SETTLE,
            ambient: 1.0,
            stars: 0.0,
            sun_size: 1.0,
        }
    }
}

/// A soft, camera-facing blob: lit and alpha-blended (dust, blown sand), or
/// self-lit and additive (plasma glow).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Particle {
    pub pos: [f32; 3],
    /// Radius, meters.
    pub size: f32,
    /// Linear rgb and alpha.
    pub color: [f32; 4],
    /// Stretched along this direction (world, length = how many times longer
    /// than wide); zero for round.
    pub stretch: [f32; 3],
    /// 0: lit and alpha-blended. Above 0: glows by itself at this strength
    /// (color times alpha), adding light rather than covering what's behind.
    pub glow: f32,
    /// 0: a smooth round blob. Above 0: a billow of smoke or flame, lumpy at
    /// the edge and through the body, its shape seeded by this (0..1].
    pub billow: f32,
}

impl Particle {
    /// Self-lit plasma light: `size` m radius, stretched along `stretch`
    /// (zero for round), at `strength` (0 is dark).
    pub fn glow(at: Vec3, size: f32, color: Vec3, strength: f32, stretch: Vec3) -> Particle {
        Particle {
            pos: at.to_array(),
            size,
            color: color.extend(1.0).to_array(),
            stretch: stretch.to_array(),
            glow: strength.max(0.0),
            billow: 0.0,
        }
    }

    /// A tongue of flame: like `glow`, but billowing, shaped by `seed` (0..1).
    pub fn fire(
        at: Vec3,
        size: f32,
        color: Vec3,
        strength: f32,
        stretch: Vec3,
        seed: f32,
    ) -> Particle {
        Particle {
            billow: billow_seed(seed),
            ..Particle::glow(at, size, color, strength, stretch)
        }
    }

    /// A lit puff of billowing smoke or dust: `size` m radius, `color` lit by
    /// the sun at `alpha` cover, its lumps shaped by `seed` (0..1).
    pub fn smoke(at: Vec3, size: f32, color: Vec3, alpha: f32, seed: f32) -> Particle {
        Particle {
            pos: at.to_array(),
            size,
            color: color.extend(alpha.clamp(0.0, 1.0)).to_array(),
            stretch: [0.0; 3],
            glow: 0.0,
            billow: billow_seed(seed),
        }
    }
}

/// A billow seed in (0, 1] from any number.
fn billow_seed(seed: f32) -> f32 {
    seed.fract().abs().max(0.001)
}

/// A light that shines from a point (a lamp): it lights whatever faces it
/// within `radius`, fading to nothing there. No shadows.
#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub pos: Vec3,
    /// Linear rgb, times how bright.
    pub color: Vec3,
    pub radius: f32,
}

/// How many point lights a frame shades with: the nearest the camera that
/// are in view.
pub const MAX_LIGHTS: usize = 32;

/// Everything the renderer needs to draw one frame.
pub struct Scene {
    pub camera: Camera,
    pub time: f32,
    pub sun_dir: Vec3,
    pub atmosphere: Atmosphere,
    /// World-space area the sun shadow map must cover.
    pub shadow_center: Vec3,
    pub shadow_radius: f32,
    pub instances: Vec<(MeshId, Instance)>,
    /// Drawn after the opaque scene, blended.
    pub particles: Vec<Particle>,
    /// Lamps lighting the scene (any number: the nearest are used).
    pub lights: Vec<PointLight>,
    /// Plasma rivers lighting it (the nearest's used).
    pub rivers: std::sync::Arc<Vec<crate::map::river::River>>,
    pub ui: Vec<UiVertex>,
    /// Draw the map's static geometry (off for studio shots like thumbnails).
    pub world: bool,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    light_view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    sky_zenith: [f32; 4],
    sky_horizon: [f32; 4],
    ground_color: [f32; 4],
    params: [f32; 4],
    /// xyz: settled dust color.
    settle: [f32; 4],
    /// x: how many of `lights` are in use.
    light_count: [f32; 4],
    /// Point lights, two vec4s each: position and radius, color.
    lights: [[f32; 4]; MAX_LIGHTS * 2],
    /// A plasma river near: how many of `river` are in use, half its width,
    /// how strong its light is.
    river_info: [f32; 4],
    /// Its course: each point, and how far down it that is.
    river: [[f32; 4]; RIVER_POINTS],
}

/// How much of a river lights a frame: this many points, `RIVER_STEP` apart
/// along its own.
pub const RIVER_POINTS: usize = 40;
const RIVER_STEP: usize = 3;

struct GpuMesh {
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    /// Indices in the full mesh, the first in `ib`.
    count: u32,
    /// A sphere round every vertex, in the mesh's own space.
    center: Vec3,
    radius: f32,
    /// Coarser levels after the full mesh in `ib`: their index ranges and how
    /// far each strays from the full mesh (mesh units), finest first.
    levels: Vec<(std::ops::Range<u32>, f32)>,
}

/// The six planes of a view-projection's frustum (wgpu depth 0..1), each
/// normalized so its distance tests are in world units.
fn frustum(m: Mat4) -> [Vec4; 6] {
    let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
    [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2].map(|p| p / p.truncate().length())
}

/// The lights to shade a frame with: those reaching into view, nearest the
/// camera first, at most `MAX_LIGHTS`.
fn nearest_lights(lights: &[PointLight], cam: Vec3, planes: &[Vec4; 6]) -> Vec<PointLight> {
    let mut seen: Vec<PointLight> = lights
        .iter()
        .filter(|l| in_frustum(planes, l.pos, l.radius))
        .copied()
        .collect();
    seen.sort_by(|a, b| {
        let d = |l: &PointLight| l.pos.distance(cam) - l.radius;
        d(a).total_cmp(&d(b))
    });
    seen.truncate(MAX_LIGHTS);
    seen
}

/// Whether a sphere at `center` of `radius` is at least partly inside `planes`.
fn in_frustum(planes: &[Vec4; 6], center: Vec3, radius: f32) -> bool {
    planes
        .iter()
        .all(|p| p.truncate().dot(center) + p.w >= -radius)
}

impl GpuMesh {
    /// A mesh and its coarser levels.
    fn new(device: &wgpu::Device, m: &MeshData, label: &str) -> GpuMesh {
        let welded = lod::Welded::new(m);
        let mut indices = m.indices.clone();
        let mut levels = Vec::new();
        for l in lod::levels(m, &welded, &m.indices, false) {
            let start = indices.len() as u32;
            indices.extend_from_slice(&l.indices);
            levels.push((start..indices.len() as u32, l.error));
        }
        let mut g = GpuMesh::plain(
            device,
            &MeshData {
                verts: m.verts.clone(),
                indices,
            },
            label,
        );
        g.count = m.indices.len() as u32;
        g.levels = levels;
        g
    }

    /// A mesh drawn whole (`levels` empty).
    fn plain(device: &wgpu::Device, m: &MeshData, label: &str) -> GpuMesh {
        let (center, radius) = bounds(m);
        GpuMesh {
            vb: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(&m.verts),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            ib: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(&m.indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            count: m.indices.len() as u32,
            center,
            radius,
            levels: Vec::new(),
        }
    }

    /// The index range of level `k` (0 is the full mesh).
    fn level(&self, k: usize) -> std::ops::Range<u32> {
        match k {
            0 => 0..self.count,
            k => self.levels[k - 1].0.clone(),
        }
    }
}

/// One ground cell of the static mesh: its triangles' indices, a sphere
/// round them, and its coarser levels.
struct Chunk {
    indices: std::ops::Range<u32>,
    center: Vec3,
    radius: f32,
    /// Index ranges of coarser copies of the cell, and how far each strays
    /// from it (meters), finest first. Each keeps the cell's edges where
    /// they are, so neighbours drawn at different levels still meet.
    levels: Vec<(std::ops::Range<u32>, f32)>,
}

impl Chunk {
    fn level(&self, k: usize) -> std::ops::Range<u32> {
        match k {
            0 => self.indices.clone(),
            k => self.levels[k - 1].0.clone(),
        }
    }
}

/// The world's static geometry ready to upload: its triangles grouped into
/// ground cells, with each cell's coarser levels after all the full cells.
/// Building it is the slow part, so it can be done off the render thread.
pub struct StaticWorld {
    mesh: MeshData,
    chunks: Vec<Chunk>,
}

impl StaticWorld {
    pub fn new(mesh: &MeshData) -> StaticWorld {
        let started = std::time::Instant::now();
        let (mut mesh, mut chunks) = chunked(mesh);
        let welded = lod::Welded::new(&mesh);
        // Cells simplify independently: a few threads share them.
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
        let per = chunks.len().div_ceil(threads).max(1);
        let levels: Vec<Vec<lod::Level>> = std::thread::scope(|scope| {
            let jobs: Vec<_> = chunks
                .chunks(per)
                .map(|part| {
                    let (mesh, welded) = (&mesh, &welded);
                    scope.spawn(move || {
                        part.iter()
                            .map(|c| {
                                let own =
                                    &mesh.indices[c.indices.start as usize..c.indices.end as usize];
                                lod::levels(mesh, welded, own, true)
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            jobs.into_iter()
                .flat_map(|j| j.join().expect("a cell's levels panicked"))
                .collect()
        });
        // Level by level, so neighbouring cells at the same level are
        // neighbours in the index buffer and draw as one.
        let deepest = levels.iter().map(Vec::len).max().unwrap_or(0);
        for k in 0..deepest {
            for (c, ls) in chunks.iter_mut().zip(&levels) {
                if let Some(l) = ls.get(k) {
                    let start = mesh.indices.len() as u32;
                    mesh.indices.extend_from_slice(&l.indices);
                    c.levels.push((start..mesh.indices.len() as u32, l.error));
                }
            }
        }
        log::debug!(
            "static world: {} cells, {} triangles, levels made in {:.2?}",
            chunks.len(),
            chunks.iter().map(|c| c.indices.len() / 3).sum::<usize>(),
            started.elapsed()
        );
        StaticWorld { mesh, chunks }
    }
}

/// Ground cell size for [`chunked`], meters.
const CHUNK: f32 = 24.0;

/// `mesh` with its triangles regrouped by the ground cell their middle is
/// in, and each cell's range.
fn chunked(mesh: &MeshData) -> (MeshData, Vec<Chunk>) {
    let pos = |i: u32| Vec3::from(mesh.verts[i as usize].pos);
    let mut cells: std::collections::BTreeMap<(i32, i32), Vec<[u32; 3]>> = Default::default();
    for t in mesh.indices.chunks_exact(3) {
        let mid = (pos(t[0]) + pos(t[1]) + pos(t[2])) / 3.0;
        let key = (
            (mid.x / CHUNK).floor() as i32,
            (mid.z / CHUNK).floor() as i32,
        );
        cells.entry(key).or_default().push([t[0], t[1], t[2]]);
    }
    let mut indices = Vec::with_capacity(mesh.indices.len());
    let mut chunks = Vec::with_capacity(cells.len());
    for tris in cells.values() {
        let start = indices.len() as u32;
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for &i in tris.iter().flatten() {
            lo = lo.min(pos(i));
            hi = hi.max(pos(i));
            indices.push(i);
        }
        let center = (lo + hi) * 0.5;
        let radius = tris
            .iter()
            .flatten()
            .map(|&i| pos(i).distance(center))
            .fold(0.0, f32::max);
        chunks.push(Chunk {
            indices: start..indices.len() as u32,
            center,
            radius,
            levels: Vec::new(),
        });
    }
    let out = MeshData {
        verts: mesh.verts.clone(),
        indices,
    };
    (out, chunks)
}

/// The middle of a mesh's box and the farthest vertex from it.
fn bounds(m: &MeshData) -> (Vec3, f32) {
    let (lo, hi) = m.verts.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), v| {
            let p = Vec3::from(v.pos);
            (lo.min(p), hi.max(p))
        },
    );
    if m.verts.is_empty() {
        return (Vec3::ZERO, 0.0);
    }
    let center = (lo + hi) * 0.5;
    let radius = m
        .verts
        .iter()
        .map(|v| Vec3::from(v.pos).distance(center))
        .fold(0.0, f32::max);
    (center, radius)
}

/// Size-dependent render targets.
struct Targets {
    hdr_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    ao_view: wgpu::TextureView,
    /// Depth for the SSAO pass.
    ao_bg: wgpu::BindGroup,
    post_bg: wgpu::BindGroup,
    /// Bloom's chain, half the frame's size and smaller, and what reads
    /// each (the frame first).
    bloom: Vec<wgpu::TextureView>,
    bloom_bgs: Vec<wgpu::BindGroup>,
}

/// How many sizes bloom's blurred at.
const BLOOM_MIPS: usize = 6;

/// How light sky `a` is: 0 at night, 1 in full day.
fn daylight(a: &Atmosphere) -> f32 {
    let sky = a.sky_horizon.lerp(a.sky_zenith, 0.35);
    let luma = sky.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    let t = ((luma - 0.06) / 0.34).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How strong bloom is under sky `a`: full at night, a touch in full sun
/// (pale ground in daylight isn't a light).
fn bloom_strength(a: &Atmosphere) -> f32 {
    let day = daylight(a);
    0.6 * (1.0 - day) + 0.012 * day
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,

    scene_pipe: wgpu::RenderPipeline,
    /// Depth-only scene geometry, drawn before `scene_pipe` so the costly
    /// shading runs once per pixel instead of once per overlapping surface.
    prepass_pipe: wgpu::RenderPipeline,
    shadow_pipe: wgpu::RenderPipeline,
    sky_pipe: wgpu::RenderPipeline,
    particle_pipe: wgpu::RenderPipeline,
    particle_buf: wgpu::Buffer,
    particle_cap: usize,
    particle_count: u32,
    post_pipe: wgpu::RenderPipeline,
    bloom_bright: wgpu::RenderPipeline,
    bloom_down: wgpu::RenderPipeline,
    bloom_up: wgpu::RenderPipeline,
    bloom_bgl: wgpu::BindGroupLayout,
    ui_pipe: wgpu::RenderPipeline,

    globals_buf: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    shadow_bg: wgpu::BindGroup,
    shadow_tex: wgpu::Texture,
    shadow_view: wgpu::TextureView,
    /// The static world's shadow depth on its own, for the light it was
    /// drawn for and the world mesh it was drawn from. In a match neither
    /// moves, so each frame copies this in and draws only what moves on top.
    shadow_cache: Option<(Mat4, u64, wgpu::Texture, wgpu::TextureView)>,
    /// Bumped each time the static world mesh changes.
    static_gen: u64,
    /// The sun's view-projection for the frame being drawn.
    light_view_proj: Mat4,
    ui_globals_buf: wgpu::Buffer,
    ui_globals_bg: wgpu::BindGroup,
    post_bgl: wgpu::BindGroupLayout,
    ao_bgl: wgpu::BindGroupLayout,
    ao_pipe: wgpu::RenderPipeline,
    linear_sampler: wgpu::Sampler,
    targets: Targets,

    meshes: Vec<GpuMesh>,
    static_mesh: Option<GpuMesh>,
    /// The static mesh's triangles, grouped into ground cells (index ranges
    /// with a bounding sphere each), so the camera draws only cells in view.
    chunks: Vec<Chunk>,
    instance_buf: wgpu::Buffer,
    instance_cap: usize,
    ui_buf: wgpu::Buffer,
    ui_cap: usize,

    /// Thumbnail atlas the UI can sample, and its bind group.
    thumb_bgl: wgpu::BindGroupLayout,
    thumb_bg: wgpu::BindGroup,
    thumb_size: (u32, u32),
    thumb_tex: Option<wgpu::Texture>,
    /// Render targets and colour texture for one thumbnail, kept between calls.
    thumb_targets: Option<((u32, u32), Targets, wgpu::Texture)>,
    /// GPU timestamps around each pass, for `--bench` (when the GPU has them).
    timer: Option<GpuTimer>,
    /// Draw distant meshes at coarser levels.
    lod: bool,
    /// Triangles the last frame drew for the camera and for the sun.
    drawn: (u64, u64),
}

/// Instance ranges to draw, by mesh and level.
type Ranges = Vec<(MeshId, usize, std::ops::Range<u32>)>;

/// The passes `encode` times, in order.
pub const PASSES: [&str; 6] = ["shadow", "prepass", "scene", "particles", "ssao", "post+ui"];

/// Timestamp queries around each pass, and the buffers to read them back.
struct GpuTimer {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    /// Nanoseconds per timestamp tick.
    period: f32,
}

impl Renderer {
    pub fn new_windowed(window: Arc<Window>) -> Result<Renderer> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window).context("create surface")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .context("no compatible GPU adapter")?;
        let (device, queue) = request_device(&adapter)?;

        let (w, h) = (size.width.max(1), size.height.max(1));
        let mut config = surface
            .get_default_config(&adapter, w, h)
            .ok_or_else(|| anyhow!("surface not supported by adapter"))?;
        let caps = surface.get_capabilities(&adapter);
        if let Some(srgb) = caps.formats.iter().copied().find(|f| f.is_srgb()) {
            config.format = srgb;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        config.desired_maximum_frame_latency = 2;
        surface.configure(&device, &config);
        let format = config.format;
        Ok(Self::build(
            device,
            queue,
            Some((surface, config)),
            format,
            w,
            h,
        ))
    }

    pub fn new_headless(width: u32, height: u32) -> Result<Renderer> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .context("no GPU adapter")?;
        let (device, queue) = request_device(&adapter)?;
        Ok(Self::build(
            device,
            queue,
            None,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            width,
            height,
        ))
    }

    fn build(
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Renderer {
        let scene_src = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene.wgsl"),
            // Material ids come from render/material.rs, never hard-coded in the shader.
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}\n{}",
                    material::Material::wgsl_constants(),
                    include_str!("../shaders/scene.wgsl")
                )
                .into(),
            ),
        });
        let post_src = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/post.wgsl").into()),
        });
        let ui_src = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/ui.wgsl").into()),
        });

        let uniform_entry = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let globals_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let shadow_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                // Procedural surface detail (see surface.rs), sampled triplanar.
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let post_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let bloom_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let ao_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ssao depth"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let ui_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::VERTEX),
                // The font's distance-field atlas and its (mipmapped) sampler.
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = |bgls: &[Option<&wgpu::BindGroupLayout>]| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: bgls,
                immediate_size: 0,
            })
        };
        let scene_layout = layout(&[Some(&globals_bgl), Some(&shadow_bgl)]);
        let globals_layout = layout(&[Some(&globals_bgl)]);
        let post_layout = layout(&[Some(&post_bgl)]);
        let bloom_layout = layout(&[Some(&bloom_bgl)]);
        let ao_layout = layout(&[Some(&globals_bgl), Some(&ao_bgl)]);
        // Particles also read scene depth, to fade out where they meet geometry.
        let particle_layout = layout(&[Some(&globals_bgl), Some(&shadow_bgl), Some(&ao_bgl)]);
        let thumb_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("thumbnails"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let ui_layout = layout(&[Some(&ui_bgl), Some(&thumb_bgl)]);

        let vertex_layouts = [
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4],
            },
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Instance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4
                ],
            },
        ];
        let vertex_layouts: Vec<Option<wgpu::VertexBufferLayout>> =
            vertex_layouts.into_iter().map(Some).collect();

        let depth_state = |write: bool, compare| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(write),
            depth_compare: Some(compare),
            stencil: Default::default(),
            bias: Default::default(),
        };

        let scene_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &scene_src,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &vertex_layouts,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            // The depth prepass has already laid down the nearest surface:
            // shade only that one.
            depth_stencil: Some(depth_state(false, wgpu::CompareFunction::GreaterEqual)),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &scene_src,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(HDR_FORMAT.into())],
            }),
            multiview_mask: None,
            cache: None,
        });

        // The same vertex shader as `scene_pipe`, so both land on exactly the
        // same depths.
        let prepass_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth prepass"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &scene_src,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &vertex_layouts,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Greater)),
            multisample: Default::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });

        let shadow_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&globals_layout),
            vertex: wgpu::VertexState {
                module: &scene_src,
                entry_point: Some("vs_shadow"),
                compilation_options: Default::default(),
                buffers: &vertex_layouts,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
                ..depth_state(true, wgpu::CompareFunction::Less)
            }),
            multisample: Default::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });

        let sky_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(&globals_layout),
            vertex: wgpu::VertexState {
                module: &scene_src,
                entry_point: Some("vs_sky"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(depth_state(false, wgpu::CompareFunction::GreaterEqual)),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &scene_src,
                entry_point: Some("fs_sky"),
                compilation_options: Default::default(),
                targets: &[Some(HDR_FORMAT.into())],
            }),
            multiview_mask: None,
            cache: None,
        });

        let particle_src = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/particle.wgsl").into()),
        });
        let particle_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particles"),
            layout: Some(&particle_layout),
            vertex: wgpu::VertexState {
                module: &particle_src,
                entry_point: Some("vs_particle"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Particle>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x4, 3 => Float32x4, 4 => Float32],
                })],
            },
            primitive: Default::default(),
            depth_stencil: Some(depth_state(false, wgpu::CompareFunction::GreaterEqual)),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &particle_src,
                entry_point: Some("fs_particle"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let ao_src = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ssao.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/ssao.wgsl").into()),
        });
        let ao_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ssao"),
            layout: Some(&ao_layout),
            vertex: wgpu::VertexState {
                module: &ao_src,
                entry_point: Some("vs_ao"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &ao_src,
                entry_point: Some("fs_ao"),
                compilation_options: Default::default(),
                targets: &[Some(AO_FORMAT.into())],
            }),
            multiview_mask: None,
            cache: None,
        });

        let post_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("post"),
            layout: Some(&post_layout),
            vertex: wgpu::VertexState {
                module: &post_src,
                entry_point: Some("vs_post"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &post_src,
                entry_point: Some("fs_post"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });

        let bloom_src = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bloom.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/bloom.wgsl").into()),
        });
        let bloom_pipe = |entry: &str, src: wgpu::BlendFactor, dst: wgpu::BlendFactor| {
            let part = wgpu::BlendComponent {
                src_factor: src,
                dst_factor: dst,
                operation: wgpu::BlendOperation::Add,
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&bloom_layout),
                vertex: wgpu::VertexState {
                    module: &bloom_src,
                    entry_point: Some("vs_bloom"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &bloom_src,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: Some(wgpu::BlendState {
                            color: part,
                            alpha: part,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        use wgpu::BlendFactor as Factor;
        // (How strong it is comes in as the blend's constant.)
        let bloom_bright = bloom_pipe("fs_bright", Factor::Constant, Factor::Zero);
        let bloom_down = bloom_pipe("fs_down", Factor::One, Factor::Zero);
        let bloom_up = bloom_pipe("fs_up", Factor::One, Factor::One);

        let ui_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&ui_layout),
            vertex: wgpu::VertexState {
                module: &ui_src,
                entry_point: Some("vs_ui"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4, 2 => Float32x2, 3 => Float32x2],
                })],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &ui_src,
                entry_point: Some("fs_ui"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buf.as_entire_binding(),
            }],
        });
        let ui_globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui globals"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let font_view = make_font_atlas(&device, &queue);
        let font_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("font"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let ui_globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui globals"),
            layout: &ui_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ui_globals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&font_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&font_sampler),
                },
            ],
        });

        let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow map"),
            size: wgpu::Extent3d {
                width: SHADOW_SIZE,
                height: SHADOW_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let shadow_view = shadow_tex.create_view(&Default::default());
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow cmp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let surface_view = upload_surface_textures(&device, &queue);
        let surface_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("surface"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let shadow_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow"),
            layout: &shadow_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&surface_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&surface_sampler),
                },
            ],
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let targets = make_targets(
            &device,
            (&post_bgl, &bloom_bgl),
            &ao_bgl,
            &linear_sampler,
            width,
            height,
        );
        let thumb_bg = make_thumb_bg(&device, &thumb_bgl, &linear_sampler, format, (1, 1)).1;
        let meshes = MeshData::primitives()
            .iter()
            .map(|m| GpuMesh::new(&device, m, "primitive"))
            .collect();
        let instance_cap = 1024;
        let instance_buf = make_buffer(
            &device,
            "instances",
            instance_cap * std::mem::size_of::<Instance>(),
            wgpu::BufferUsages::VERTEX,
        );
        let particle_cap = 1024;
        let particle_buf = make_buffer(
            &device,
            "particles",
            particle_cap * std::mem::size_of::<Particle>(),
            wgpu::BufferUsages::VERTEX,
        );
        let ui_cap = 4096;
        let ui_buf = make_buffer(
            &device,
            "ui",
            ui_cap * std::mem::size_of::<UiVertex>(),
            wgpu::BufferUsages::VERTEX,
        );

        Renderer {
            device,
            queue,
            surface,
            format,
            width,
            height,
            scene_pipe,
            prepass_pipe,
            shadow_pipe,
            sky_pipe,
            particle_pipe,
            particle_buf,
            particle_cap,
            particle_count: 0,
            post_pipe,
            bloom_bright,
            bloom_down,
            bloom_up,
            bloom_bgl,
            ui_pipe,
            globals_buf,
            globals_bg,
            shadow_bg,
            shadow_tex,
            shadow_view,
            shadow_cache: None,
            static_gen: 0,
            light_view_proj: Mat4::IDENTITY,
            ui_globals_buf,
            ui_globals_bg,
            post_bgl,
            ao_bgl,
            ao_pipe,
            linear_sampler,
            targets,
            meshes,
            static_mesh: None,
            chunks: Vec::new(),
            lod: !lod::disabled(),
            drawn: (0, 0),
            instance_buf,
            instance_cap,
            ui_buf,
            ui_cap,
            thumb_bgl,
            thumb_bg,
            thumb_size: (1, 1),
            thumb_tex: None,
            thumb_targets: None,
            timer: None,
        }
    }

    /// Make the thumbnail atlas `size` px (cleared) if it isn't already.
    /// Returns whether it was remade, so anything drawn into it is gone.
    pub fn ensure_thumb_atlas(&mut self, size: (u32, u32)) -> bool {
        let size = (size.0.max(1), size.1.max(1));
        if size == self.thumb_size {
            return false;
        }
        let (tex, bg) = make_thumb_bg(
            &self.device,
            &self.thumb_bgl,
            &self.linear_sampler,
            self.format,
            size,
        );
        self.thumb_tex = Some(tex);
        self.thumb_bg = bg;
        self.thumb_size = size;
        true
    }

    /// Render `scene` at `size` px into the thumbnail atlas at `origin`.
    pub fn render_thumb(&mut self, scene: &Scene, origin: (u32, u32), size: (u32, u32)) {
        let Some(atlas) = self.thumb_tex.clone() else {
            return;
        };
        if self
            .thumb_targets
            .as_ref()
            .is_none_or(|(s, _, _)| *s != size)
        {
            let targets = make_targets(
                &self.device,
                (&self.post_bgl, &self.bloom_bgl),
                &self.ao_bgl,
                &self.linear_sampler,
                size.0,
                size.1,
            );
            let color = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("thumbnail"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            self.thumb_targets = Some((size, targets, color));
        }
        let (_, mut targets, color) = self.thumb_targets.take().expect("made above");
        // Render as if the screen were the thumbnail's size.
        std::mem::swap(&mut self.targets, &mut targets);
        let screen = (self.width, self.height);
        (self.width, self.height) = size;
        let view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("thumbnail"),
            });
        self.encode(scene, &view, &mut encoder);
        encoder.copy_texture_to_texture(
            color.as_image_copy(),
            wgpu::TexelCopyTextureInfo {
                texture: &atlas,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: origin.0,
                    y: origin.1,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: size.0.min(self.thumb_size.0 - origin.0),
                height: size.1.min(self.thumb_size.1 - origin.1),
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        (self.width, self.height) = screen;
        std::mem::swap(&mut self.targets, &mut targets);
        self.thumb_targets = Some((size, targets, color));
    }

    /// Register a mesh for instanced drawing.
    pub fn add_mesh(&mut self, mesh: &MeshData) -> MeshId {
        self.meshes.push(GpuMesh::new(&self.device, mesh, "mesh"));
        MeshId(self.meshes.len() as u32 - 1)
    }

    /// Replace a registered mesh with one whose detail is in its vertex
    /// colors: drawn in full however far off (coarser copies would smear the paint).
    pub fn replace_painted(&mut self, id: MeshId, mesh: &MeshData) {
        self.meshes[id.0 as usize] = GpuMesh::plain(&self.device, mesh, "painted");
    }

    /// Upload world geometry that never moves (drawn with an identity transform).
    pub fn set_static_mesh(&mut self, mesh: &MeshData) {
        self.set_static_world(StaticWorld::new(mesh));
    }

    /// `set_static_mesh` with the slow part already done.
    pub fn set_static_world(&mut self, world: StaticWorld) {
        self.chunks = world.chunks;
        let mut gpu = GpuMesh::plain(&self.device, &world.mesh, "static");
        // Only the full cells count: the shadow cache draws them all.
        gpu.count = self.chunks.iter().map(|c| c.indices.end).max().unwrap_or(0);
        self.static_mesh = Some(gpu);
        self.static_gen += 1;
    }

    /// Triangles drawn in the last frame: for the camera (prepass and scene
    /// pass each draw these), and into the sun's shadow map.
    pub fn drawn(&self) -> (u64, u64) {
        self.drawn
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        if let Some((surface, config)) = &mut self.surface {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }
        self.targets = make_targets(
            &self.device,
            (&self.post_bgl, &self.bloom_bgl),
            &self.ao_bgl,
            &self.linear_sampler,
            width,
            height,
        );
    }

    /// Blurs the bright of the frame, for the post pass to add.
    fn bloom(&self, encoder: &mut wgpu::CommandEncoder, strength: f32) {
        let t = &self.targets;
        let mut pass = |from: usize, to: usize, pipe: &wgpu::RenderPipeline, keep: bool| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.bloom[to],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if keep {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipe);
            let s = strength as f64;
            pass.set_blend_constant(wgpu::Color {
                r: s,
                g: s,
                b: s,
                a: 1.0,
            });
            pass.set_bind_group(0, &t.bloom_bgs[from], &[]);
            pass.draw(0..3, 0..1);
        };
        pass(0, 0, &self.bloom_bright, false);
        for k in 1..BLOOM_MIPS {
            pass(k, k, &self.bloom_down, false);
        }
        for k in (1..BLOOM_MIPS).rev() {
            pass(k + 1, k - 1, &self.bloom_up, true);
        }
    }

    /// Render to the window surface.
    pub fn render(&mut self, scene: &Scene) {
        let Some((surface, config)) = &self.surface else {
            return;
        };
        let asked = std::time::Instant::now();
        let got = surface.get_current_texture();
        let waited = asked.elapsed();
        let outcome = match &got {
            wgpu::CurrentSurfaceTexture::Success(_) => None,
            wgpu::CurrentSurfaceTexture::Suboptimal(_) => Some("suboptimal"),
            wgpu::CurrentSurfaceTexture::Timeout => Some("timed out"),
            wgpu::CurrentSurfaceTexture::Occluded => Some("occluded"),
            wgpu::CurrentSurfaceTexture::Outdated => Some("outdated"),
            wgpu::CurrentSurfaceTexture::Lost => Some("lost"),
            _ => Some("failed"),
        };
        // A window that holds back its next image stalls the whole frame: say so.
        // (Minimized it's occluded every frame, and that's expected.)
        if waited > SLOW_ACQUIRE || outcome.is_some_and(|o| !matches!(o, "suboptimal" | "occluded"))
        {
            crate::hitch::report(format_args!(
                "window image {} after {:.0} ms",
                outcome.unwrap_or("acquired"),
                waited.as_secs_f64() * 1e3
            ));
        }
        let tex = match got {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(&self.device, config);
                return;
            }
            _ => return,
        };
        let view = tex
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        self.encode(scene, &view, &mut encoder);
        self.queue.submit([encoder.finish()]);
        self.queue.present(tex);
    }

    /// Triangles in the static world mesh, and in one draw of `scene`'s instances.
    pub fn triangles(&self, scene: &Scene) -> (u32, u64) {
        let world = self.static_mesh.as_ref().map_or(0, |m| m.count / 3);
        let moving = scene
            .instances
            .iter()
            .map(|(id, _)| (self.meshes[id.0 as usize].count / 3) as u64)
            .sum();
        (world, moving)
    }

    /// Time each pass on the GPU from now on, if it can; false if it can't.
    pub fn enable_gpu_timer(&mut self) -> bool {
        if !self
            .device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            return false;
        }
        let count = 2 * PASSES.len() as u32;
        let size = count as u64 * 8;
        self.timer = Some(GpuTimer {
            queries: self.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("pass times"),
                ty: wgpu::QueryType::Timestamp,
                count,
            }),
            resolve: self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pass times"),
                size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pass times readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            period: self.queue.get_timestamp_period(),
        });
        true
    }

    fn timestamps(&self, pass: u32) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        self.timer
            .as_ref()
            .map(|t| wgpu::RenderPassTimestampWrites {
                query_set: &t.queries,
                beginning_of_pass_write_index: Some(2 * pass),
                end_of_pass_write_index: Some(2 * pass + 1),
            })
    }

    /// Render offscreen and wait for the GPU to finish (the `--bench` timer).
    /// Returns the CPU time spent encoding and submitting, then the time
    /// spent waiting on the GPU after that.
    pub fn render_timed(
        &mut self,
        scene: &Scene,
    ) -> Result<(std::time::Duration, std::time::Duration)> {
        let start = std::time::Instant::now();
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bench"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("bench"),
            });
        self.encode(scene, &view, &mut encoder);
        if let Some(t) = &self.timer {
            encoder.resolve_query_set(&t.queries, 0..2 * PASSES.len() as u32, &t.resolve, 0);
            encoder.copy_buffer_to_buffer(&t.resolve, 0, &t.readback, 0, None);
        }
        self.queue.submit([encoder.finish()]);
        let cpu = start.elapsed();
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| anyhow!("poll: {e:?}"))?;
        Ok((cpu, start.elapsed() - cpu))
    }

    /// Each pass's GPU time in milliseconds for the last [`Renderer::render_timed`]
    /// (0 for a pass that was skipped), if the timer is on.
    pub fn pass_times(&self) -> Result<Option<[f32; PASSES.len()]>> {
        let Some(t) = &self.timer else {
            return Ok(None);
        };
        t.readback
            .map_async(wgpu::MapMode::Read, .., |r| r.expect("map pass times"));
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| anyhow!("poll: {e:?}"))?;
        let mut out = [0.0; PASSES.len()];
        {
            let data = t
                .readback
                .get_mapped_range(..)
                .map_err(|e| anyhow!("map: {e:?}"))?;
            let ticks: &[u64] = bytemuck::cast_slice(&data);
            for (i, o) in out.iter_mut().enumerate() {
                let (a, b) = (ticks[2 * i], ticks[2 * i + 1]);
                *o = b.saturating_sub(a) as f32 * t.period / 1e6;
            }
        }
        t.readback.unmap();
        Ok(Some(out))
    }

    /// Render offscreen and read the result back as an image.
    pub fn render_to_image(&mut self, scene: &Scene) -> Result<image::RgbaImage> {
        let (w, h) = (self.width, self.height);
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("shot"),
            });
        self.encode(scene, &view, &mut encoder);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        readback.map_async(wgpu::MapMode::Read, .., |r| r.expect("map readback"));
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| anyhow!("poll: {e:?}"))?;
        let data = readback
            .get_mapped_range(..)
            .map_err(|e| anyhow!("map: {e:?}"))?;
        let mut img = image::RgbaImage::new(w, h);
        for y in 0..h {
            let src = &data[(y * row) as usize..(y * row + w * 4) as usize];
            img.as_mut()[(y * w * 4) as usize..((y + 1) * w * 4) as usize].copy_from_slice(src);
        }
        Ok(img)
    }

    /// Upload this frame's instances, particles, UI and globals. Returns the
    /// instance ranges to draw for the camera, then for the sun's shadow map:
    /// each holds only the instances inside that view.
    fn upload(&mut self, scene: &Scene) -> (Ranges, Ranges, Vec<std::ops::Range<u32>>) {
        let (w, h) = (self.width as f32, self.height as f32);
        let view_proj = scene.camera.view_proj(w / h);
        let sun = scene.sun_dir.normalize();
        let r = scene.shadow_radius;
        let light_view = look_at_mat4(
            scene.shadow_center + sun * (r * 2.0),
            scene.shadow_center,
            Vec3::Y,
        );
        let light_proj = proj::orthographic(-r, r, -r, r, 1.0, r * 4.0);
        let light_view_proj = light_proj * light_view;

        // Slot 0 is the identity instance for the static mesh; then what the
        // camera sees, then what the sun sees, each grouped by mesh.
        let mut sorted: Vec<&(MeshId, Instance)> = scene.instances.iter().collect();
        sorted.sort_by_key(|(id, _)| *id);
        let spheres: Vec<(Vec3, f32)> = sorted
            .iter()
            .map(|(id, inst)| {
                let m = &self.meshes[id.0 as usize];
                let model = Mat4::from_cols_array_2d(&inst.model);
                let scale = model
                    .x_axis
                    .truncate()
                    .length()
                    .max(model.y_axis.truncate().length())
                    .max(model.z_axis.truncate().length());
                (model.transform_point3(m.center), m.radius * scale)
            })
            .collect();
        let mut data = Vec::with_capacity(2 * sorted.len() + 1);
        data.push(Instance::new(Mat4::IDENTITY, Vec4::new(1.0, 1.0, 1.0, 0.0)));
        // How far a level may stray (meters) for the camera at a distance,
        // and anywhere in the shadow map. Off, nothing may.
        let lod = self.lod;
        let fov = scene.camera.fov_y;
        let cam = scene.camera.pos;
        let on_screen = |center: Vec3, radius: f32| {
            let d = center.distance(cam) - radius;
            if !lod || d <= CAM_NEAR {
                return 0.0;
            }
            lod::PIXEL_ERROR / lod::pixels_per_meter(h, fov, d)
        };
        let in_shadow = if lod {
            lod::TEXEL_ERROR * 2.0 * r / SHADOW_SIZE as f32
        } else {
            0.0
        };
        let meshes = &self.meshes;
        let mut drawn = (0u64, 0u64);
        let mut pick = |planes: [Vec4; 6], allowed: &dyn Fn(Vec3, f32) -> f32, tris: &mut u64| {
            let mut keep: Vec<(MeshId, usize, &Instance)> = Vec::new();
            for ((id, inst), &(c, rad)) in sorted.iter().zip(&spheres) {
                if !in_frustum(&planes, c, rad) {
                    continue;
                }
                let m = &meshes[id.0 as usize];
                let scale = rad / m.radius.max(1e-6);
                let k = lod::pick(m.levels.iter().map(|l| l.1), scale, allowed(c, rad));
                *tris += (m.level(k).len() / 3) as u64;
                keep.push((*id, k, inst));
            }
            keep.sort_by_key(|&(id, k, _)| (id, k));
            let mut ranges = Ranges::new();
            for (id, k, inst) in keep {
                let i = data.len() as u32;
                match ranges.last_mut() {
                    Some((last, lk, r)) if *last == id && *lk == k && r.end == i => r.end = i + 1,
                    _ => ranges.push((id, k, i..i + 1)),
                }
                data.push(*inst);
            }
            ranges
        };
        let seen = pick(frustum(view_proj), &on_screen, &mut drawn.0);
        // A caster's shadow falls near it, so it may not stray more than the
        // camera would notice there either (or a near tank's shadow on
        // itself would change).
        let cast = |c: Vec3, rad: f32| in_shadow.min(on_screen(c, rad));
        let shadowed = pick(frustum(light_view_proj), &cast, &mut drawn.1);
        self.light_view_proj = light_view_proj;
        // The static world's cells in view, each at the coarsest level that
        // passes for it, neighbours merged into one draw.
        let planes = frustum(view_proj);
        let mut world: Vec<std::ops::Range<u32>> = Vec::new();
        if scene.world {
            for c in &self.chunks {
                if !in_frustum(&planes, c.center, c.radius) {
                    continue;
                }
                let k = lod::pick(
                    c.levels.iter().map(|l| l.1),
                    1.0,
                    on_screen(c.center, c.radius),
                );
                let range = c.level(k);
                drawn.0 += (range.len() / 3) as u64;
                match world.last_mut() {
                    Some(r) if r.end == range.start => r.end = range.end,
                    _ => world.push(range),
                }
            }
        }
        self.drawn = drawn;
        if data.len() > self.instance_cap {
            self.instance_cap = data.len().next_power_of_two();
            self.instance_buf = make_buffer(
                &self.device,
                "instances",
                self.instance_cap * std::mem::size_of::<Instance>(),
                wgpu::BufferUsages::VERTEX,
            );
        }
        self.queue
            .write_buffer(&self.instance_buf, 0, bytemuck::cast_slice(&data));

        if scene.particles.len() > self.particle_cap {
            self.particle_cap = scene.particles.len().next_power_of_two();
            self.particle_buf = make_buffer(
                &self.device,
                "particles",
                self.particle_cap * std::mem::size_of::<Particle>(),
                wgpu::BufferUsages::VERTEX,
            );
        }
        if !scene.particles.is_empty() {
            self.queue.write_buffer(
                &self.particle_buf,
                0,
                bytemuck::cast_slice(&scene.particles),
            );
        }
        self.particle_count = scene.particles.len() as u32;

        if scene.ui.len() > self.ui_cap {
            self.ui_cap = scene.ui.len().next_power_of_two();
            self.ui_buf = make_buffer(
                &self.device,
                "ui",
                self.ui_cap * std::mem::size_of::<UiVertex>(),
                wgpu::BufferUsages::VERTEX,
            );
        }
        if !scene.ui.is_empty() {
            self.queue
                .write_buffer(&self.ui_buf, 0, bytemuck::cast_slice(&scene.ui));
        }

        let mut globals = Globals {
            view_proj: view_proj.to_cols_array_2d(),
            light_view_proj: light_view_proj.to_cols_array_2d(),
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            cam_pos: scene.camera.pos.extend(1.0).to_array(),
            sun_dir: sun.extend(scene.atmosphere.sun_size).to_array(),
            // .w channels carry the wind (x, z) and how much sand it blows.
            sun_color: scene
                .atmosphere
                .sun_color
                .extend(scene.atmosphere.wind.x)
                .to_array(),
            sky_zenith: scene
                .atmosphere
                .sky_zenith
                .extend(scene.atmosphere.wind.y)
                .to_array(),
            sky_horizon: scene
                .atmosphere
                .sky_horizon
                .extend(scene.atmosphere.blowing_sand)
                .to_array(),
            ground_color: scene
                .atmosphere
                .ground_color
                .extend(scene.atmosphere.ambient)
                .to_array(),
            params: [scene.time, scene.atmosphere.fog_density, w, h],
            settle: scene
                .atmosphere
                .settle
                .extend(scene.atmosphere.stars)
                .to_array(),
            light_count: [0.0; 4],
            lights: [[0.0; 4]; MAX_LIGHTS * 2],
            river_info: [0.0; 4],
            river: [[0.0; 4]; RIVER_POINTS],
        };
        if let Some(r) = scene.rivers.first() {
            let near = r.stretch(scene.camera.pos, RIVER_POINTS, RIVER_STEP);
            let night = 1.0 - daylight(&scene.atmosphere);
            globals.river_info = [near.len() as f32, r.half, 0.3 + 0.7 * night, night];
            for (g, p) in globals.river.iter_mut().zip(&near) {
                *g = p.to_array();
            }
        }
        let near = nearest_lights(&scene.lights, scene.camera.pos, &frustum(view_proj));
        globals.light_count[0] = near.len() as f32;
        for (k, l) in near.iter().enumerate() {
            globals.lights[k * 2] = l.pos.extend(l.radius).to_array();
            globals.lights[k * 2 + 1] = l.color.extend(0.0).to_array();
        }
        self.queue
            .write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
        self.queue.write_buffer(
            &self.ui_globals_buf,
            0,
            bytemuck::cast_slice(&[w, h, 0.0, 0.0]),
        );
        (seen, shadowed, world)
    }

    fn draw_geometry<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        ranges: &[(MeshId, usize, std::ops::Range<u32>)],
        world: &[std::ops::Range<u32>],
    ) {
        pass.set_vertex_buffer(1, self.instance_buf.slice(..));
        if let Some(m) = self.static_mesh.as_ref().filter(|_| !world.is_empty()) {
            pass.set_vertex_buffer(0, m.vb.slice(..));
            pass.set_index_buffer(m.ib.slice(..), wgpu::IndexFormat::Uint32);
            for r in world {
                pass.draw_indexed(r.clone(), 0, 0..1);
            }
        }
        let mut bound = None;
        for (id, k, r) in ranges {
            let m = &self.meshes[id.0 as usize];
            if bound != Some(*id) {
                pass.set_vertex_buffer(0, m.vb.slice(..));
                pass.set_index_buffer(m.ib.slice(..), wgpu::IndexFormat::Uint32);
                bound = Some(*id);
            }
            pass.draw_indexed(m.level(*k), 0, r.clone());
        }
    }

    /// Draw the static world's shadow depth into the cache, unless it's
    /// already there for this light and this world.
    fn refresh_shadow_cache(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let key = (self.light_view_proj, self.static_gen);
        if self
            .shadow_cache
            .as_ref()
            .is_some_and(|(m, g, _, _)| (*m, *g) == key)
        {
            return;
        }
        let (tex, view) = match self.shadow_cache.take() {
            Some((_, _, tex, view)) => (tex, view),
            None => {
                let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("static shadow"),
                    size: wgpu::Extent3d {
                        width: SHADOW_SIZE,
                        height: SHADOW_SIZE,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: DEPTH_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let view = tex.create_view(&Default::default());
                (tex, view)
            }
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("static shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_pipe);
            pass.set_bind_group(0, &self.globals_bg, &[]);
            let all = self.static_mesh.as_ref().map_or(0, |m| m.count);
            self.draw_geometry(&mut pass, &[], &[0..all]);
        }
        self.shadow_cache = Some((key.0, key.1, tex, view));
    }

    fn encode(
        &mut self,
        scene: &Scene,
        out: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let (seen, shadowed, world) = self.upload(scene);

        // The world's own shadow comes from the cache when it still fits.
        let cached = scene.world && self.static_mesh.is_some();
        if cached {
            self.refresh_shadow_cache(encoder);
            let (_, _, tex, _) = self.shadow_cache.as_ref().expect("just refreshed");
            encoder.copy_texture_to_texture(
                tex.as_image_copy(),
                self.shadow_tex.as_image_copy(),
                wgpu::Extent3d {
                    width: SHADOW_SIZE,
                    height: SHADOW_SIZE,
                    depth_or_array_layers: 1,
                },
            );
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: if cached {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(1.0)
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.timestamps(0),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_pipe);
            pass.set_bind_group(0, &self.globals_bg, &[]);
            // Uncached, the sun sees the whole world.
            let all = self.static_mesh.as_ref().map_or(0, |m| m.count);
            let sun_world = if scene.world && !cached {
                vec![0..all]
            } else {
                Vec::new()
            };
            self.draw_geometry(&mut pass, &shadowed, &sun_world);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("depth prepass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CAM_DEPTH_CLEAR),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.timestamps(1),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.prepass_pipe);
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_bind_group(1, &self.shadow_bg, &[]);
            self.draw_geometry(&mut pass, &seen, &world);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.hdr_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.timestamps(2),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.scene_pipe);
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_bind_group(1, &self.shadow_bg, &[]);
            self.draw_geometry(&mut pass, &seen, &world);
            pass.set_pipeline(&self.sky_pipe);
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.draw(0..3, 0..1);
        }

        // Particles in their own pass with depth read-only, so they can also
        // sample it and fade out where they cut into geometry.
        if self.particle_count > 0 {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("particles"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.hdr_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.depth_view,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                timestamp_writes: self.timestamps(3),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.particle_pipe);
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_bind_group(1, &self.shadow_bg, &[]);
            pass.set_bind_group(2, &self.targets.ao_bg, &[]);
            pass.set_vertex_buffer(0, self.particle_buf.slice(..));
            pass.draw(0..6, 0..self.particle_count);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ssao"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.ao_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: self.timestamps(4),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.ao_pipe);
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_bind_group(1, &self.targets.ao_bg, &[]);
            pass.draw(0..3, 0..1);
        }

        self.bloom(encoder, bloom_strength(&scene.atmosphere));

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: out,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: self.timestamps(5),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.post_pipe);
            pass.set_bind_group(0, &self.targets.post_bg, &[]);
            pass.draw(0..3, 0..1);
            if !scene.ui.is_empty() {
                pass.set_pipeline(&self.ui_pipe);
                pass.set_bind_group(0, &self.ui_globals_bg, &[]);
                pass.set_bind_group(1, &self.thumb_bg, &[]);
                pass.set_vertex_buffer(0, self.ui_buf.slice(..));
                pass.draw(0..scene.ui.len() as u32, 0..1);
            }
        }
    }
}

fn make_buffer(
    device: &wgpu::Device,
    label: &str,
    size: usize,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as u64,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Generate the procedural surface textures and upload them as a mipmapped array.
fn upload_surface_textures(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let layers = surface::generate();
    let size = surface::SIZE as u32;
    let mip_count = layers[0].len() as u32;
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("surface"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: surface::LAYERS,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, mips) in layers.iter().enumerate() {
        for (level, data) in mips.iter().enumerate() {
            let w = (size >> level).max(1);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(w),
                },
                wgpu::Extent3d {
                    width: w,
                    height: w,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    tex.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

/// The UI font's distance-field atlas, all mips uploaded.
fn make_font_atlas(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let font = crate::ui::font::font();
    let (w, h) = font.atlas_size;
    let levels = font.atlas_pixels();
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("font atlas"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, data) in levels.iter().enumerate() {
        let (lw, lh) = (w >> level, h >> level);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(lw),
                rows_per_image: Some(lh),
            },
            wgpu::Extent3d {
                width: lw,
                height: lh,
                depth_or_array_layers: 1,
            },
        );
    }
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// A thumbnail atlas of `size` px and the UI bind group that samples it.
fn make_thumb_bg(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    format: wgpu::TextureFormat,
    size: (u32, u32),
) -> (wgpu::Texture, wgpu::BindGroup) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("thumbnail atlas"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("thumbnails"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    (tex, bg)
}

fn make_targets(
    device: &wgpu::Device,
    (post_bgl, bloom_bgl): (&wgpu::BindGroupLayout, &wgpu::BindGroupLayout),
    ao_bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    width: u32,
    height: u32,
) -> Targets {
    let tex = |label, format, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let hdr = tex(
        "hdr",
        HDR_FORMAT,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let depth = tex(
        "depth",
        DEPTH_FORMAT,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let ao = tex(
        "ssao",
        AO_FORMAT,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let hdr_view = hdr.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    let ao_view = ao.create_view(&Default::default());
    let ao_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ssao depth"),
        layout: ao_bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&depth_view),
        }],
    });
    let bloom: Vec<wgpu::TextureView> = (0..BLOOM_MIPS)
        .map(|k| {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bloom"),
                size: wgpu::Extent3d {
                    width: (width >> (k + 1)).max(1),
                    height: (height >> (k + 1)).max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            t.create_view(&Default::default())
        })
        .collect();
    let bloom_bgs = std::iter::once(&hdr_view)
        .chain(&bloom)
        .map(|view| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom"),
                layout: bloom_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
        })
        .collect();
    let post_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("post"),
        layout: post_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&hdr_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&ao_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&bloom[0]),
            },
        ],
    });
    Targets {
        hdr_view,
        depth_view,
        ao_view,
        ao_bg,
        post_bg,
        bloom,
        bloom_bgs,
    }
}

fn request_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue)> {
    let info = adapter.get_info();
    log::info!("GPU: {} ({:?})", info.name, info.backend);
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("tankit"),
        // Timestamps only feed `--bench`; ask for them where they're free to have.
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .context("request device")
}

impl mesh::MeshStore for Renderer {
    fn add_mesh(&mut self, mesh: &MeshData) -> MeshId {
        Renderer::add_mesh(self, mesh)
    }
}
