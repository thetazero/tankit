//! `--arena`: bot-only rounds played headless, as fast as the sim runs,
//! to measure the bots. Every side runs the frozen baseline brain
//! (`ai::baseline`) except the one named by `--improve`, which runs the
//! current one; it prints each round's score and each side's wins.

use std::time::Instant;

use crate::combat::{Match, Phase};
use crate::faction::Faction;

pub fn run(args: &crate::Args, rounds: u32, improve: Option<Faction>) -> anyhow::Result<()> {
    let sides = args.map().factions;
    let names: Vec<&str> = sides.iter().map(|f| f.name()).collect();
    println!(
        "{} rounds of {} ({:?}); current bots: {}",
        rounds,
        names.join(" v "),
        args.contract,
        improve.map_or("nobody", |f| f.name()),
    );
    let mut wins = vec![0u32; sides.len()];
    let mut draws = 0;
    let mut totals = vec![0u32; sides.len()];
    // Kills and deaths per (side, gun), summed over every round.
    let mut by_gun: std::collections::BTreeMap<(usize, &str), (u32, u32, u32)> = Default::default();
    // With --front: every round a battle in this galaxy's war.
    let mut war = args.front.then(|| crate::galaxy::Galaxy::warmed(args.seed));
    for r in 0..rounds as u64 {
        let seed = args.seed.wrapping_add(r * 1_000_003);
        let mut m = Match::new(args.map(), (r as usize) % 4, args.bots, seed);
        m.set_contract(args.contract);
        m.bots_only(seed);
        for (team, &f) in sides.iter().enumerate() {
            if Some(f) != improve {
                m.baseline_side(team, seed);
            }
        }
        let battle = war
            .as_mut()
            .map(|g| (crate::galaxy::battle::deploy(g, &mut m), g.census()));
        // Time the sim alone, not building the map.
        let (clock, mut steps) = (Instant::now(), 0u32);
        while m.phase == Phase::Playing {
            m.step(Default::default(), false);
            m.sounds.clear();
            steps += 1;
        }
        let ms_per_step = clock.elapsed().as_secs_f64() * 1e3 / steps.max(1) as f64;
        for (f, t) in m.fighters.iter().zip(&m.world.tanks) {
            let e = by_gun
                .entry((f.team, crate::garage::turret_name(t.turret_kind)))
                .or_default();
            e.0 += f.kills;
            e.1 += f.deaths;
            e.2 += 1;
        }
        let scores: Vec<u32> = (0..sides.len()).map(|t| m.score(t)).collect();
        for (t, s) in scores.iter().enumerate() {
            totals[t] += s;
        }
        match m.leader() {
            Some(t) => wins[t] += 1,
            None => draws += 1,
        }
        if let (Some(g), Some((b, before))) = (war.as_mut(), battle) {
            println!(
                "  {} ({:.0} s left): allocation {:?}, lost {:?}, left {:?}",
                crate::galaxy::battle::system_name(b.system),
                m.time_left,
                b.allocation,
                m.lost(),
                m.hulls.as_ref().unwrap(),
            );
            let s = crate::galaxy::battle::conclude(g, &b, &m);
            println!(
                "  {}",
                crate::galaxy::battle::describe(&b, &s).replace('\n', "\n  ")
            );
            let after = g.census();
            for &f in &b.sides {
                println!(
                    "  galaxy: {:<9} hulls {:8.0} -> {:8.0}  share {:4.1}% -> {:4.1}%  (tick {} -> {})",
                    Faction::ALL[f].name(),
                    before.hulls[f],
                    after.hulls[f],
                    before.share[f] * 100.0,
                    after.share[f] * 100.0,
                    before.tick,
                    after.tick,
                );
            }
        }
        println!(
            "round {r:>3} seed {seed:>12}: {}  ({:.0} s, {ms_per_step:.3} ms a step)",
            scores
                .iter()
                .zip(&names)
                .map(|(s, n)| format!("{n} {s:>3}"))
                .collect::<Vec<_>>()
                .join("  "),
            clock.elapsed().as_secs_f32(),
        );
    }
    println!("wins:");
    for (t, n) in names.iter().enumerate() {
        println!(
            "  {n:<10} {:>3}  ({:.0}%), score {:.1} a round",
            wins[t],
            100.0 * wins[t] as f32 / rounds as f32,
            totals[t] as f32 / rounds as f32
        );
    }
    println!("  draws      {draws:>3}");
    println!("by gun (kills / deaths, tanks fielded):");
    for ((team, gun), (k, d, n)) in by_gun {
        println!(
            "  {:<10} {gun:<8} {k:>4} / {d:<4} ({n})  k/d {:.2}",
            names[team],
            k as f32 / d.max(1) as f32
        );
    }
    Ok(())
}

/// `--duel`: one tank against one on open ground, `start` m apart, over
/// and over: (hull, gun) `a` under the current brain against `b` under the
/// baseline, then `a` under the baseline too, for comparison. Prints how
/// often `a` comes out on top: kills `b` (as a husk, too) before it's
/// wrecked itself.
pub fn duel(
    a: (crate::sim::HullKind, crate::sim::TurretKind),
    b: (crate::sim::HullKind, crate::sim::TurretKind),
    start: f32,
    reps: u32,
) -> anyhow::Result<()> {
    use glam::Vec2;
    for current in [true, false] {
        let (mut wins, mut losses, mut time) = (0, 0, 0.0);
        let mut dealt = 0.0;
        let (mut shots, mut hits) = (0u32, 0u32);
        for r in 0..reps as u64 {
            let mut m = Match::new(crate::map::Map::empty(500.0), 0, 1, r);
            m.refit_bot(0, a.0, a.1, current, r * 31 + 1);
            m.refit_bot(1, b.0, b.1, false, r * 31 + 2);
            let yaw = r as f32 * 0.7;
            let off = Vec2::new(yaw.sin(), yaw.cos()) * start * 0.5;
            m.world.teleport(0, Vec2::splat(250.0) - off, yaw);
            m.world.teleport(1, Vec2::splat(250.0) + off, yaw + 3.0);
            for f in &mut m.fighters {
                f.shield = 0.0;
            }
            let mut t = 0.0;
            // Over at the first kill; a broken Vigil fights on as a husk
            // first, and can still take its killer with it.
            while t < 60.0 && m.fighters.iter().all(|f| f.alive) && m.fighters[1].deaths == 0 {
                m.step(Default::default(), false);
                for snd in &m.sounds {
                    match snd {
                        crate::combat::Sound::Shot { tank: 0, .. } => shots += 1,
                        crate::combat::Sound::Hit { victim: 1, .. } => hits += 1,
                        _ => {}
                    }
                }
                m.sounds.clear();
                t += crate::game::DT;
            }
            time += t;
            dealt += m.fighters[0].damage_dealt;
            if m.fighters[1].deaths > 0 {
                wins += 1;
            } else if t < 60.0 {
                losses += 1;
            }
        }
        println!(
            "{} brain: {wins} won, {losses} lost, {} timed out; {:.1} s a duel, {:.0} damage dealt a duel, {shots} shots, {hits} hits",
            if current { "current " } else { "baseline" },
            reps - wins - losses,
            time / reps as f32,
            dealt / reps as f32
        );
    }
    Ok(())
}
