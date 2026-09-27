//! Experiment 16, from a terminal.
//!
//! ```text
//!   combat serve [--port N] [--host A]         the encounter, in a browser
//!   combat play  [--nominal N] [--seed S] [--trace K]
//!                                              one wave, played headlessly, and the
//!                                              four questions the experiment asks
//!   combat scale [--seed S]                    100 to 1,000,000 attackers, timed
//! ```
//!
//! `play` is the acceptance command. It sends one wave at the defence, lets the
//! fight run until the domain settles, repairs what the fight tore up, and then
//! asks:
//!
//! ```text
//!   what did the fight cost, in records rather than attackers?
//!   which sectors woke, which were recompiled, and are the rest untouched?
//!   what was written down, against what happened?
//!   do the live run, a replay and a resumed checkpoint agree?
//! ```
//!
//! `scale` is the success criterion in the brief: the same encounter at four
//! (and here five) sizes, with runtime put next to the number of distinct
//! cohort states rather than next to the number of attackers.

use std::time::Instant;
use temporal_rooms::combat::field::{BATTERIES, SECTORS, STRUCTURES};
use temporal_rooms::combat::run::{Checkpoint, Cmd, Encounter};
use temporal_rooms::combat::{clock, commas, TICK_RATE};
use temporal_rooms::model::Tick;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("play");
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    let seed = flag(rest, "--seed").and_then(|s| s.parse().ok()).unwrap_or(16);
    let code = match cmd {
        "serve" => {
            let port = flag(rest, "--port").and_then(|s| s.parse().ok()).unwrap_or(8800);
            let host = flag(rest, "--host").unwrap_or("127.0.0.1");
            match temporal_rooms::combat::net::serve(host, port) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("cannot serve on {host} port {port}: {e}");
                    1
                }
            }
        }
        "play" => play(
            seed,
            flag(rest, "--nominal")
                .and_then(|s| s.replace(',', "").parse().ok())
                .unwrap_or(DEFAULT_WAVE),
            flag(rest, "--trace").and_then(|s| s.parse().ok()).unwrap_or(28),
        ),
        "scale" => scale(seed),
        other => {
            eprintln!("`{other}` is not a command. Try `serve`, `play` or `scale`.");
            2
        }
    };
    std::process::exit(code);
}

/// Big enough to breach the outer defences and tear up the conveyor, small
/// enough that the inner batteries finish it.
pub const DEFAULT_WAVE: u64 = 3_000;

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn rule(title: &str) {
    println!("\n\x1b[1m{title}\x1b[0m");
    println!("{}", "-".repeat(title.len().max(8)));
}

/// Run until the domain has opened and closed again, or give up.
fn settle(e: &mut Encounter, cap: Tick, mut each: impl FnMut(&Encounter)) {
    let stop = e.now + cap;
    while e.now < stop {
        e.advance_to(e.now + TICK_RATE);
        each(e);
        if !e.open {
            break;
        }
    }
}

fn play(seed: u64, nominal: u64, trace: usize) -> i32 {
    let mut e = Encounter::new(seed);
    e.fight.trace = Some(Vec::new());
    e.advance_to(2 * TICK_RATE);
    if let Err(m) = e.apply(Cmd::Wave { nominal }) {
        eprintln!("{m}");
        return 1;
    }
    let sent = e.now;

    // The height of it: the moment the most records were live.
    let mut peak = (0usize, e.overlay(), 0);
    let mut mid: Option<Checkpoint> = None;
    let t0 = Instant::now();
    settle(&mut e, 20 * 60 * TICK_RATE, |e| {
        let live = e.fight.cohorts.len() + e.fight.volleys.len();
        if live > peak.0 {
            peak = (live, e.overlay(), e.now);
        }
        if mid.is_none() && e.open {
            mid = e.checkpoints.iter().find(|c| c.tick > sent).cloned();
        }
    });
    let fought = t0.elapsed();
    let closed_at = e.episodes.last().and_then(|ep| ep.closed);
    let lost = e.fight.lost_structures();

    // Afterwards: repair what the fight took from the factory.
    e.advance_to(e.now + 5 * TICK_RATE);
    let mut repaired = Vec::new();
    for (s, def) in STRUCTURES.iter().enumerate() {
        if def.tie.is_some() && e.fight.hp_at(s, e.now) < def.hp * 1000 {
            let _ = e.apply(Cmd::Repair { structure: s });
            repaired.push(def.name);
        }
    }
    e.advance_to(e.now + 20 * TICK_RATE);
    let end = e.now;

    rule(&format!("experiment 16: one wave of {} against the works, seed {seed}", commas(nominal as u128)));
    let f = &e.fight;
    println!("  sent at {}   domain settled at {}", clock(sent), closed_at.map(clock).unwrap_or("-".into()));
    println!(
        "  killed {}   got through {}   structures lost: {}",
        commas(f.killed as u128),
        commas(f.leaked as u128),
        if lost.is_empty() { "none".into() } else { lost.join(", ") }
    );
    println!(
        "  {} events, {} volleys, {} splits, {} merges, {} cohort records ever made, {:.1} ms",
        commas(f.stats.events as u128),
        commas(f.stats.volleys as u128),
        commas(f.stats.splits as u128),
        commas(f.stats.merges as u128),
        commas(f.stats.created as u128),
        fought.as_secs_f64() * 1000.0
    );

    rule("the first events, as the fight saw them -- none of them written down");
    if let Some(tr) = &f.trace {
        for line in tr.iter().take(trace) {
            println!("  {line}");
        }
        if tr.len() > trace {
            println!("  ... {} more in the trace, {} in the fight", tr.len() - trace, commas(f.stats.events as u128));
        }
    }

    rule(&format!("COMBAT DOMAIN at {}, the height of it", clock(peak.2)));
    let o = &peak.1;
    let n = |k: &str| o.at(k).as_u64().unwrap_or(0) as u128;
    println!("  Nominal entities:    {:>10}", commas(n("nominal")));
    println!(
        "    attackers {}, turrets {}, shells in flight {}, structures {}, machines {}",
        commas(n("attackers")),
        n("turrets"),
        n("shells"),
        n("structures"),
        commas(n("machines"))
    );
    println!("  Compressed state:");
    println!("    Enemy cohorts      {:>6}", n("cohorts"));
    println!("    Turret populations {:>6}", n("batteries"));
    println!("    Projectiles        {:>6}", n("volleys"));
    println!("    Unique structures  {:>6}", n("structures"));
    println!("    Sector cells       {:>6}", n("sectorCells"));
    let secs = (closed_at.unwrap_or(end) - sent) as f64 / TICK_RATE as f64;
    println!(
        "  Events processed/s  {:>6.0} simulated, {:.0} wall",
        f.stats.events as f64 / secs.max(1.0),
        f.stats.events as f64 / fought.as_secs_f64().max(1e-9)
    );

    rule("what the encounter said");
    for (t, s) in &e.notes {
        println!("  {}  {s}", clock(*t));
    }

    rule("the disturbance boundary");
    let mut calm = Encounter::new(seed);
    calm.advance_to(end);
    let same = e.same_sectors(&calm, end);
    println!("  {:<10} {:<8} {:>5} {:>10} {:>7} {:>9}  {}", "sector", "domain", "woke", "stepped", "evals", "recompile", "against a world with no fight");
    for (i, s) in e.sectors.iter().enumerate() {
        let verdict = if same[i] {
            "identical, bit for bit".to_string()
        } else {
            let ch = s.changed();
            format!("differs: {}", if ch.is_empty() { "lost output while down".into() } else { ch.join(", ") })
        };
        println!(
            "  {:<10} {:<8} {:>5} {:>10} {:>7} {:>9}  {}",
            s.spec.name,
            if Encounter::inside(i) { "inside" } else { "outside" },
            s.woke,
            commas(s.stepped as u128),
            s.evals.get(),
            s.recompiles,
            verdict
        );
    }
    for s in &e.sectors {
        for (t, what) in &s.edits {
            println!("    {} {}  {}", clock(*t), s.spec.name, what);
        }
        for (t, sc) in &s.scrap {
            println!("    {} {}  scrap: {} -- {}", clock(*t), s.spec.name, sc.what, sc.detail);
        }
    }
    if !repaired.is_empty() {
        println!("  repaired afterwards: {}", repaired.join(", "));
    }
    println!(
        "  every sector closed again: {}   ({} of {} in closed form now)",
        e.closed_count() == SECTORS.len(),
        e.closed_count(),
        SECTORS.len()
    );

    rule("what was written down");
    let log = e.log_json().to_string();
    println!("  {log}");
    println!(
        "  {} bytes of log for {} derived events -- {} serialized",
        log.len(),
        commas(f.stats.events as u128),
        0
    );
    let cks = e.checkpoints_taken;
    println!(
        "  {} checkpoints, {} bytes on average; the fight's part of the latest is {} bytes",
        cks,
        commas((e.checkpoint_bytes / cks.max(1)) as u128),
        e.fight.to_json().to_string().len()
    );

    rule("three reconstructions of the same tick");
    let live = e.hash();
    let replay = Encounter::replay(seed, &e.log, end).hash();
    println!("  live                          {live:016x}");
    println!("  replayed from the seed        {replay:016x}");
    let mut good = replay == live;
    match &mid {
        Some(cp) => match Encounter::resume(seed, cp, &e.log, end) {
            Ok(r) => {
                let h = r.hash();
                println!("  resumed from {} (mid-fight)   {h:016x}", clock(cp.tick));
                good &= h == live;
            }
            Err(m) => {
                println!("  resumed from {}: {m}", clock(cp.tick));
                good = false;
            }
        },
        None => println!("  (no checkpoint fell inside the fight)"),
    }
    println!("  {}", if good { "all agree" } else { "THEY DISAGREE" });
    if good {
        0
    } else {
        1
    }
}

fn scale(seed: u64) -> i32 {
    rule("the same encounter, five sizes");
    println!(
        "  {:>10} {:>5} {:>5} {:>7} {:>7} {:>8} {:>9} {:>9} {:>4} {:>3} {:>5} {:>8} {:>8} {:>9}",
        "attackers", "pkts", "peak", "records", "events", "visits", "killed", "through", "lost", "rec", "sim", "runtime", "us/event", "ns/visit"
    );
    let mut rows = Vec::new();
    for n in [100u64, 1_000, 10_000, 100_000, 1_000_000] {
        let mut e = Encounter::new(seed);
        e.advance_to(TICK_RATE);
        let _ = e.apply(Cmd::Wave { nominal: n });
        let packets = e.fight.pending.len();
        let start = e.now;
        let t0 = Instant::now();
        settle(&mut e, 30 * 60 * TICK_RATE, |_| {});
        let wall = t0.elapsed().as_secs_f64();
        let f = &e.fight;
        let rec: u32 = e.sectors.iter().map(|s| s.recompiles).sum();
        println!(
            "  {:>10} {:>5} {:>5} {:>7} {:>7} {:>8} {:>9} {:>9} {:>4} {:>3} {:>5} {:>6.1}ms {:>8.2} {:>9.1}",
            commas(n as u128),
            packets,
            f.stats.peak_cohorts,
            f.stats.created,
            commas(f.stats.events as u128),
            commas(f.stats.visits as u128),
            commas(f.killed as u128),
            commas(f.leaked as u128),
            f.lost_structures().len(),
            rec,
            clock(e.now - start),
            wall * 1000.0,
            wall * 1e6 / f.stats.events.max(1) as f64,
            wall * 1e9 / f.stats.visits.max(1) as f64
        );
        rows.push((n, f.stats.peak_cohorts, f.stats.events, wall));
    }
    let (first, last) = (rows[0], rows[rows.len() - 1]);
    println!();
    println!(
        "  {}x the attackers, {:.1}x the peak cohorts, {:.1}x the events, {:.1}x the runtime.",
        commas((last.0 / first.0) as u128),
        last.1 as f64 / first.1.max(1) as f64,
        last.2 as f64 / first.2.max(1) as f64,
        last.3 / first.3.max(1e-9)
    );
    println!(
        "  A cohort is one record whatever it stands for; a battery is one record for {} turrets.",
        BATTERIES.iter().map(|b| b.count).sum::<u32>()
    );
    println!("  `visits` is records looked at per event, summed: the work, which is what the runtime follows.");
    0
}
