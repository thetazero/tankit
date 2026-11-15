mod app;
mod game;
mod render;

use anyhow::{Context, Result, bail};

const USAGE: &str = "\
tankit [OPTIONS]

  (no options)            play in a window
  --shot <out.png>        render one frame offscreen and save it
  --time <seconds>        simulated time before the shot (default 1)
  --size <WxH>            shot resolution (default 1600x900)
";

struct ShotArgs {
    out: String,
    time: f32,
    width: u32,
    height: u32,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn"))
        .init();
    match parse_args()? {
        Some(shot) => screenshot(shot),
        None => app::run(),
    }
}

fn parse_args() -> Result<Option<ShotArgs>> {
    let mut args = std::env::args().skip(1);
    let mut shot: Option<ShotArgs> = None;
    let mut time = 1.0;
    let (mut width, mut height) = (1600, 900);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--shot" => {
                shot = Some(ShotArgs { out: args.next().context("--shot needs a path")?, time, width, height })
            }
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
    Ok(shot.map(|s| ShotArgs { time, width, height, ..s }))
}

fn screenshot(args: ShotArgs) -> Result<()> {
    let mut game = game::Game::default();
    let steps = (args.time / game::DT).round() as u32;
    for _ in 0..steps {
        game.step();
    }
    let mut renderer = render::Renderer::new_headless(args.width, args.height)?;
    let img = renderer.render_to_image(&render::Frame { time: game.time() })?;
    img.save(&args.out).with_context(|| format!("save {}", args.out))?;
    println!("wrote {} ({}x{}, t={:.2}s)", args.out, args.width, args.height, game.time());
    Ok(())
}
