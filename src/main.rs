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
  --shot <out.png>        render one frame offscreen and save it
  --time <seconds>        simulated time before the shot (default 1)
  --size <WxH>            shot resolution (default 1600x900)
";

struct Args {
    seed: u64,
    shot: Option<ShotArgs>,
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
    match args.shot {
        Some(shot) => screenshot(args.seed, shot),
        None => app::run(args.seed),
    }
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args().skip(1);
    let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos() as u64 % 100_000;
    let mut shot: Option<ShotArgs> = None;
    let mut time = 1.0;
    let (mut width, mut height) = (1600, 900);
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
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("unknown argument {other}\n\n{USAGE}"),
        }
    }
    Ok(Args { seed, shot: shot.map(|s| ShotArgs { time, width, height, ..s }) })
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
    let frame = scene::build(&world, camera, args.width as f32, args.height as f32);
    let img = renderer.render_to_image(&frame)?;
    img.save(&args.out).with_context(|| format!("save {}", args.out))?;
    println!("wrote {} ({}x{}, seed {seed}, t={:.2}s)", args.out, args.width, args.height, world.time);
    Ok(())
}
