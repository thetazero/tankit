mod app;
mod game;
mod map;
mod render;
mod rng;
mod scene;
mod showroom;
mod sim;
mod tank;

use anyhow::{Context, Result, bail};
use glam::Vec3;

const USAGE: &str = "\
tankit [OPTIONS]

  (no options)            play in a window
  --seed <n>              map seed (default: random)
  --shot <out.png>        render one frame offscreen and save it
  --time <seconds>        simulated time before the shot (default 1)
  --size <WxH>            shot resolution (default 1600x900)
  --showroom              all hull x turret combos on an open yard, orbiting camera
  --orbit <degrees>       showroom: fixed camera angle instead of orbiting
  --dist <meters>         showroom: camera distance (default 24)
  --focus <0-3>           showroom: frame one tank
";

struct Args {
    seed: u64,
    shot: Option<ShotArgs>,
    showroom: Option<ShowroomArgs>,
}

#[derive(Clone, Copy)]
pub struct ShowroomArgs {
    pub orbit: Option<f32>,
    pub dist: f32,
    pub focus: Option<usize>,
}

struct ShotArgs {
    out: String,
    time: f32,
    width: u32,
    height: u32,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn"))
        .init();
    let args = parse_args()?;
    match (args.shot, args.showroom) {
        (Some(shot), Some(room)) => showroom_shot(args.seed, shot, room),
        (Some(shot), None) => screenshot(args.seed, shot),
        (None, room) => app::run(args.seed, room),
    }
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args().skip(1);
    let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos() as u64 % 100_000;
    let mut shot: Option<ShotArgs> = None;
    let mut time = 1.0;
    let (mut width, mut height) = (1600, 900);
    let mut showroom: Option<ShowroomArgs> = None;
    let mut room = ShowroomArgs { orbit: None, dist: 24.0, focus: None };
    while let Some(a) = args.next() {
        match a.as_str() {
            "--shot" => {
                shot = Some(ShotArgs { out: args.next().context("--shot needs a path")?, time, width, height })
            }
            "--seed" => seed = args.next().context("--seed needs a number")?.parse()?,
            "--time" => time = args.next().context("--time needs seconds")?.parse()?,
            "--size" => {
                let s = args.next().context("--size needs WxH")?;
                let (w, h) = s.split_once('x').context("--size format is WxH")?;
                (width, height) = (w.parse()?, h.parse()?);
            }
            "--showroom" => showroom = Some(room),
            "--orbit" => room.orbit = Some(args.next().context("--orbit needs degrees")?.parse::<f32>()?.to_radians()),
            "--dist" => room.dist = args.next().context("--dist needs meters")?.parse()?,
            "--focus" => room.focus = Some(args.next().context("--focus needs a tank index")?.parse()?),
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("unknown argument {other}\n\n{USAGE}"),
        }
    }
    Ok(Args { seed, shot: shot.map(|s| ShotArgs { time, width, height, ..s }), showroom: showroom.map(|_| room) })
}

fn screenshot(seed: u64, args: ShotArgs) -> Result<()> {
    let mut world = sim::World::new(seed);
    let mut cam = scene::ChaseCam::new();
    let steps = (args.time / game::DT).round() as u32;
    let mut camera = cam.update(&world, &world.tanks[0], game::DT);
    for _ in 0..steps {
        world.step(&[game::autopilot(world.time)]);
        camera = cam.update(&world, &world.tanks[0], game::DT);
    }
    let mut renderer = render::Renderer::new_headless(args.width, args.height)?;
    renderer.set_static_mesh(&scene::map_mesh(&world.map));
    let art = tank::TankArt::upload(&mut renderer);
    let frame = scene::build(&world, &art, camera, args.width as f32, args.height as f32);
    let img = renderer.render_to_image(&frame)?;
    img.save(&args.out).with_context(|| format!("save {}", args.out))?;
    println!("wrote {} ({}x{}, seed {seed}, t={:.2}s)", args.out, args.width, args.height, world.time);
    Ok(())
}

fn showroom_shot(seed: u64, args: ShotArgs, room: ShowroomArgs) -> Result<()> {
    let mut world = showroom::world(seed);
    let steps = (args.time / game::DT).round() as u32;
    for _ in 0..steps {
        let cmds = showroom::commands(&world);
        world.step(&cmds);
        showroom::after_step(&mut world);
    }
    let mut renderer = render::Renderer::new_headless(args.width, args.height)?;
    renderer.set_static_mesh(&showroom::ground_mesh());
    let art = tank::TankArt::upload(&mut renderer);
    let angle = room.orbit.unwrap_or(world.time * 0.15 + 0.6);
    let camera = showroom::camera(&world, angle, room.dist, room.focus);
    let mut frame = scene::build(&world, &art, camera, args.width as f32, args.height as f32);
    frame.ui.clear();
    frame.shadow_center = Vec3::ZERO;
    frame.shadow_radius = 22.0;
    let img = renderer.render_to_image(&frame)?;
    img.save(&args.out).with_context(|| format!("save {}", args.out))?;
    println!("wrote {} (showroom, t={:.2}s)", args.out, world.time);
    Ok(())
}
