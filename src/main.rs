mod app;
mod game;
mod map;
mod render;
mod rng;
mod scene;
mod sim;

use anyhow::{Context, Result, bail};

const USAGE: &str = "\
tankit [OPTIONS]

  (no options)            play in a window
  --seed <n>              map seed (default: random)
  --map <city|proving>    city (default) or the physics test course
  --spawn <n>             which spawn point to start at
  --straight              (with --shot) full throttle straight ahead instead of the weaving autopilot
  --shot <out.png>        render one frame offscreen and save it
  --time <seconds>        simulated time before the shot (default 1)
  --size <WxH>            shot resolution (default 1600x900)
  --frames <n>            (with --shot) render n frames from --time on, saved as <out>_000.png ...
  --fps <f>               frame rate for --frames (default 30)
";

struct Args {
    seed: u64,
    proving: bool,
    spawn: usize,
    straight: bool,
    shot: Option<ShotArgs>,
}

impl Args {
    fn map(&self) -> map::Map {
        if self.proving { map::Map::proving_ground() } else { map::Map::generate(self.seed) }
    }
}

struct ShotArgs {
    out: String,
    time: f32,
    width: u32,
    height: u32,
    frames: u32,
    fps: f32,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn"))
        .init();
    let args = parse_args()?;
    match &args.shot {
        Some(shot) => screenshot(&args, shot),
        None => app::run(args.seed, args.proving, args.spawn),
    }
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args().skip(1);
    let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos() as u64 % 100_000;
    let mut shot: Option<ShotArgs> = None;
    let mut time = 1.0;
    let (mut width, mut height) = (1600, 900);
    let (mut frames, mut fps) = (1u32, 30.0f32);
    let (mut proving, mut spawn, mut straight) = (false, 0usize, false);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--shot" => {
                shot = Some(ShotArgs { out: args.next().context("--shot needs a path")?, time, width, height, frames, fps })
            }
            "--seed" => seed = args.next().context("--seed needs a number")?.parse()?,
            "--map" => match args.next().as_deref() {
                Some("city") => proving = false,
                Some("proving") => proving = true,
                other => bail!("--map expects city or proving, got {other:?}"),
            },
            "--straight" => straight = true,
            "--frames" => frames = args.next().context("--frames needs a count")?.parse()?,
            "--fps" => fps = args.next().context("--fps needs a number")?.parse()?,
            "--spawn" => spawn = args.next().context("--spawn needs a number")?.parse()?,
            "--time" => time = args.next().context("--time needs seconds")?.parse()?,
            "--size" => {
                let s = args.next().context("--size needs WxH")?;
                let (w, h) = s.split_once('x').context("--size format is WxH")?;
                (width, height) = (w.parse()?, h.parse()?);
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("unknown argument {other}\n\n{USAGE}"),
        }
    }
    Ok(Args { seed, proving, spawn, straight, shot: shot.map(|s| ShotArgs { time, width, height, frames, fps, ..s }) })
}

fn screenshot(args: &Args, shot: &ShotArgs) -> Result<()> {
    let mut world = sim::World::new(args.map(), args.spawn);
    let mut cam = scene::ChaseCam::new();
    let mut renderer = render::Renderer::new_headless(shot.width, shot.height)?;
    renderer.set_static_mesh(&scene::map_mesh(&world.map));
    let command = |world: &sim::World| {
        if args.straight { sim::Command { throttle: 1.0, ..Default::default() } } else { game::autopilot(world.time) }
    };

    // Simulate `seconds` more, keeping the camera in step; returns the latest camera.
    let advance = |world: &mut sim::World, cam: &mut scene::ChaseCam, seconds: f32| {
        let mut camera = cam.update(world, &world.tanks[0], game::DT);
        for _ in 0..(seconds / game::DT).round() as u32 {
            let cmd = command(world);
            world.step(&[cmd]);
            camera = cam.update(world, &world.tanks[0], game::DT);
        }
        camera
    };
    let mut camera = advance(&mut world, &mut cam, shot.time);

    for i in 0..shot.frames {
        if i > 0 {
            camera = advance(&mut world, &mut cam, 1.0 / shot.fps);
        }
        let frame = scene::build(&world, camera, shot.width as f32, shot.height as f32);
        let img = renderer.render_to_image(&frame)?;
        let out = if shot.frames > 1 {
            let stem = shot.out.trim_end_matches(".png");
            format!("{stem}_{i:03}.png")
        } else {
            shot.out.clone()
        };
        img.save(&out).with_context(|| format!("save {out}"))?;
    }
    let t = &world.tanks[0];
    println!(
        "wrote {} frame(s) to {} ({}x{}, t={:.2}s) tank at {:.1?} up {:.2?} wheels down {}",
        shot.frames, shot.out, shot.width, shot.height, world.time, t.pos, t.up(), t.wheels_down
    );
    Ok(())
}
