//! `--bench`: play a round headless the way the window does (60 frames a
//! second, the fixed sim steps each frame needs, the scene, HUD, GPU and
//! sound) and time each part of the frame, so a change to any of them can be
//! measured before and after.

use std::time::{Duration, Instant};

use anyhow::Result;

use crate::{audio, combat, game, hud, render, scene, tank};

/// The frame rate the window is paced at (vsync on a 60 Hz panel).
const FPS: f32 = 60.0;
const RATE: u32 = 48_000;

/// What one part of the frame cost, frame by frame.
#[derive(Default)]
struct Timer {
    name: &'static str,
    samples: Vec<Duration>,
}

impl Timer {
    fn new(name: &'static str) -> Timer {
        Timer {
            name,
            samples: Vec::new(),
        }
    }

    fn report(&self) -> String {
        let mut ms: Vec<f64> = self
            .samples
            .iter()
            .map(|d| d.as_secs_f64() * 1e3)
            .collect();
        ms.sort_by(f64::total_cmp);
        let at = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
        let mean = ms.iter().sum::<f64>() / ms.len().max(1) as f64;
        format!(
            "{:<10} mean {:>6.2}  p50 {:>6.2}  p95 {:>6.2}  p99 {:>6.2}  max {:>6.2} ms",
            self.name,
            mean,
            at(0.5),
            at(0.95),
            at(0.99),
            ms.last().copied().unwrap_or(0.0),
        )
    }
}

/// Run `seconds` of a round after a `warmup`, then print what each part of
/// the frame cost.
pub fn run(
    mut game: combat::Match,
    width: u32,
    height: u32,
    warmup: f32,
    seconds: f32,
) -> Result<()> {
    let mut renderer = render::Renderer::new_headless(width, height)?;
    renderer.set_static_mesh(&scene::map_mesh(&game.world.map));
    let art = tank::TankArt::upload(&mut renderer);
    let timed = renderer.enable_gpu_timer();
    let mut passes: Vec<Timer> = render::PASSES.iter().map(|&n| Timer::new(n)).collect();
    let mut hud = hud::Hud::new();
    let mut cam = scene::ChaseCam::new();
    let mut sound = audio::Audio::offline(RATE);
    let mut pcm = vec![0.0f32; 2 * (RATE as f32 / FPS) as usize];
    let mut clock = game::Clock::default();

    let mut sim = Timer::new("sim");
    let mut build = Timer::new("scene");
    let mut ui = Timer::new("hud");
    let mut encode = Timer::new("encode");
    let mut gpu = Timer::new("gpu wait");
    let mut voice = Timer::new("audio");
    let mut total = Timer::new("frame");
    let mut steps_max = 0;
    let (mut instances, mut particles) = (0usize, 0usize);

    let frames = ((warmup + seconds) * FPS) as u32;
    let warm = (warmup * FPS) as u32;
    let dt = 1.0 / FPS;
    for frame in 0..frames {
        let start = Instant::now();
        let steps = clock.advance(dt);
        for _ in 0..steps {
            let cmd = game::autopilot(game.world.time);
            game.step(cmd, true);
        }
        let t_sim = start.elapsed();

        let alpha = clock.alpha();
        let tanks = game.display_tanks(alpha);
        let anchor = game.camera_tank(0, alpha);
        let camera = cam.update(&game.world, &anchor, dt);
        let mut frame_scene = scene::build(&game.world, &tanks, &art, camera);
        game.effect_instances(alpha, &mut frame_scene.instances);
        game.ambience.focus = camera.pos;
        frame_scene.particles.extend(game.ambience.particles());
        game.glow_particles(alpha, &mut frame_scene.particles);
        let t_scene = start.elapsed();

        let (w, h) = (width as f32, height as f32);
        frame_scene.ui = hud.draw(&game, &tanks, &camera, w, h, dt, false).verts;
        let t_hud = start.elapsed();

        let (cpu, wait) = renderer.render_timed(&frame_scene)?;
        let t_render = start.elapsed();
        let pass_ms = renderer.pass_times()?;

        let sounds = std::mem::take(&mut game.sounds);
        sound.update(&game, &sounds, &camera, true, dt);
        sound.render(&mut pcm);
        let t_audio = start.elapsed();

        if frame >= warm {
            sim.samples.push(t_sim);
            build.samples.push(t_scene - t_sim);
            ui.samples.push(t_hud - t_scene);
            encode.samples.push(cpu);
            gpu.samples.push(wait);
            voice.samples.push(t_audio - t_render);
            total.samples.push(t_audio);
            for (t, &ms) in passes.iter_mut().zip(pass_ms.iter().flatten()) {
                t.samples.push(Duration::from_secs_f32(ms / 1e3));
            }
            steps_max = steps_max.max(steps);
            instances = instances.max(frame_scene.instances.len());
            particles = particles.max(frame_scene.particles.len());
        }
    }

    let alive = game.fighters.iter().filter(|f| f.alive).count();
    println!(
        "{} tanks ({alive} alive at the end), {width}x{height}, {seconds} s after {warmup} s, \
         up to {steps_max} sim steps a frame, up to {instances} instances and {particles} particles",
        game.world.tanks.len(),
    );
    for t in [&sim, &build, &ui, &encode, &gpu, &voice, &total] {
        println!("  {}", t.report());
    }
    if timed {
        println!("  GPU time by pass:");
        for t in &passes {
            println!("    {}", t.report());
        }
    }
    let over = total
        .samples
        .iter()
        .filter(|d| d.as_secs_f64() > 1.0 / FPS as f64)
        .count();
    println!(
        "  {over} of {} frames over the {:.1} ms budget",
        total.samples.len(),
        1e3 / FPS
    );
    Ok(())
}
