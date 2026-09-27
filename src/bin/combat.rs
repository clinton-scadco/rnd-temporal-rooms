//! Experiment 16, from a terminal.
//!
//! ```text
//!   combat serve [--port N] [--host A]         the district, in a browser
//!   combat play  [--rate T] [--trace K]        an afternoon of shipping, headlessly,
//!                                              and the questions the experiment asks
//!   combat scale                               the same afternoon at five volumes, timed
//! ```
//!
//! `play` is the acceptance command. It opens both fractures, lets strain do
//! what it does, anchors the gantry and launders the ore half-way through,
//! closes the lanes, lets the district settle, repairs what was torn up, and
//! then asks:
//!
//! ```text
//!   what tore, when, and was the tick solved or found?
//!   what did the disturbance cost, in records rather than manifestations?
//!   which sectors woke, which were recompiled, and are the rest untouched?
//!   what was written down, against what happened?
//!   do the live run, a replay and a resumed checkpoint agree?
//! ```

use std::time::Instant;
use temporal_rooms::combat::field::{self, LANES, SECTORS, SITES, STRUCTURES};
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
            flag(rest, "--rate").and_then(|s| s.replace(',', "").parse().ok()).unwrap_or(DEFAULT_RATE),
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

/// Tonnes a second on the deep lane; the near lane carries half.
pub const DEFAULT_RATE: u64 = 40;

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn rule(title: &str) {
    println!("\n\x1b[1m{title}\x1b[0m");
    println!("{}", "-".repeat(title.len().max(8)));
}

/// The afternoon every run of this binary plays: open both lanes, anchor and
/// launder at `respond`, close the lanes at `stop`. Returns the commands, so a
/// test can play the same one.
pub fn afternoon(rate: u64) -> Vec<(Tick, Cmd)> {
    let s = |x: Tick| x * TICK_RATE;
    vec![
        (s(2), Cmd::Stream { lane: 0, rate }),
        (s(2), Cmd::Stream { lane: 1, rate: (rate / 2).max(1) }),
        (s(45), Cmd::Anchor { anchor: 0, on: true }),
        (s(45), Cmd::Launder { on: true }),
        (s(80), Cmd::Stream { lane: 0, rate: 0 }),
        (s(80), Cmd::Stream { lane: 1, rate: 0 }),
    ]
}

fn run_script(e: &mut Encounter, script: &[(Tick, Cmd)], mut each: impl FnMut(&Encounter)) {
    for (t, c) in script {
        while e.now < *t {
            let step = (e.now + TICK_RATE).min(*t);
            e.advance_to(step);
            each(e);
        }
        if let Err(m) = e.apply(c.clone()) {
            eprintln!("  {}: {m}", clock(e.now));
        }
    }
}

/// Run until the district is quiet and the domain has closed, or give up.
fn settle(e: &mut Encounter, cap: Tick, mut each: impl FnMut(&Encounter)) {
    let stop = e.now + cap;
    while e.now < stop {
        e.advance_to(e.now + TICK_RATE);
        each(e);
        if !e.open && e.fight.quiet() {
            break;
        }
    }
}

fn play(seed: u64, rate: u64, trace: usize) -> i32 {
    let mut e = Encounter::new(seed);
    e.fight.trace = Some(Vec::new());
    let script = afternoon(rate);

    let mut peak = (0usize, e.overlay(), 0);
    let mut mid: Option<Checkpoint> = None;
    let mut dark_ticks: Tick = 0;
    let t0 = Instant::now();
    let mut watch = |e: &Encounter| {
        let live = e.fight.cohorts.len() + e.fight.volleys.len();
        if live > peak.0 {
            peak = (live, e.overlay(), e.now);
        }
        if mid.is_none() && e.open && e.fight.alive_count() > 0 {
            mid = e.checkpoints.last().filter(|c| c.tick > 2 * TICK_RATE).cloned();
        }
        if e.fight.dark() && e.fight.lanes.iter().any(|&r| r > 0) {
            dark_ticks += TICK_RATE;
        }
    };
    run_script(&mut e, &script, &mut watch);
    settle(&mut e, 20 * 60 * TICK_RATE, &mut watch);
    let fought = t0.elapsed();
    let settled_at = e.episodes.last().and_then(|ep| ep.closed);
    let lost = e.fight.lost_structures();

    e.advance_to(e.now + 5 * TICK_RATE);
    let mut repaired = Vec::new();
    for (s, def) in STRUCTURES.iter().enumerate() {
        if e.fight.hp_at(s, e.now) < def.hp * 1000 && e.apply(Cmd::Repair { structure: s }).is_ok() {
            repaired.push(def.name);
        }
    }
    e.advance_to(e.now + 20 * TICK_RATE);
    let end = e.now;
    let f = &e.fight;

    rule(&format!(
        "experiment 16: an afternoon at the gantry, {} t/s of 1890 ore and {} t/s of 2070 lathes",
        commas(rate as u128),
        commas((rate / 2).max(1) as u128)
    ));
    for (t, c) in &script {
        let what = match c {
            Cmd::Stream { lane, rate } if *rate == 0 => format!("close the {}", LANES[*lane].name.to_lowercase()),
            Cmd::Stream { lane, rate } => format!("open the {} at {} t/s", LANES[*lane].name.to_lowercase(), rate),
            Cmd::Anchor { anchor, .. } => format!("power the {}", field::ANCHORS[*anchor].name.to_lowercase()),
            Cmd::Launder { .. } => "launder the ore".into(),
            other => format!("{other:?}"),
        };
        println!("  {}  {what}", clock(*t));
    }
    let crossed: Vec<String> = (0..LANES.len())
        .map(|l| format!("{} t of {}", commas((f.crossed_at(l, end) / 1000) as u128), LANES[l].item))
        .collect();
    println!("  crossed: {}", crossed.join(", "));
    println!(
        "  manifested {}   destroyed {}   faded {}   structures lost: {}",
        commas(f.spawned as u128),
        commas(f.killed as u128),
        commas(f.faded as u128),
        if lost.is_empty() { "none".into() } else { lost.join(", ") }
    );
    println!(
        "  {} tears, {} events, {} volleys, {} splits, {} merges, {} cohort records ever made, {:.1} ms",
        f.stats.tears,
        commas(f.stats.events as u128),
        commas(f.stats.volleys as u128),
        commas(f.stats.splits as u128),
        commas(f.stats.merges as u128),
        commas(f.stats.created as u128),
        fought.as_secs_f64() * 1000.0
    );
    println!(
        "  the interface was dark for {} of the time it was asked to carry something; settled at {}",
        clock(dark_ticks),
        settled_at.map(clock).unwrap_or("-".into())
    );

    rule("strain, where it ended up");
    println!("  {:<12} {:>12} {:>12} {:>12}  {}", "site", "1890 ty", "2070 ty", "tears at", "holds");
    for (s, def) in SITES.iter().enumerate() {
        let st = f.strain_at(s, end);
        println!(
            "  {:<12} {:>12} {:>12} {:>12}  {}",
            def.name,
            commas((st[0] / field::TY) as u128),
            commas((st[1] / field::TY) as u128),
            commas((def.open / field::TY) as u128),
            def.holds.map(|c| field::ORIGINS[c].tag()).unwrap_or("nothing: a door")
        );
    }

    rule("the first events, as the disturbance saw them -- none of them written down");
    if let Some(tr) = &f.trace {
        for line in tr.iter().take(trace) {
            println!("  {line}");
        }
        if tr.len() > trace {
            println!("  ... {} more in the trace, {} in the run", tr.len() - trace, commas(f.stats.events as u128));
        }
    }

    rule(&format!("DISTURBANCE DOMAIN at {}, the height of it", clock(peak.2)));
    let o = &peak.1;
    let n = |k: &str| o.at(k).as_u64().or_else(|| o.at(k).as_str().and_then(|s| s.parse().ok())).unwrap_or(0) as u128;
    println!("  Nominal entities:    {:>10}", commas(n("nominal")));
    println!(
        "    manifestations {}, turrets {}, shells in flight {}, structures {}, machines awake {}",
        commas(n("manifest")),
        n("turrets"),
        n("shells"),
        n("structures"),
        commas(n("machines"))
    );
    println!("  Compressed state:");
    println!("    Manifest cohorts   {:>6}", n("cohorts"));
    println!("    Open ruptures      {:>6}", n("ruptures"));
    println!("    Strain records     {:>6}", n("strainRecords"));
    println!("    Turret populations {:>6}", n("batteries"));
    println!("    Projectiles        {:>6}", n("volleys"));
    println!("    Sector cells       {:>6}", n("sectorCells"));

    rule("what the district said");
    for (t, s) in &e.notes {
        println!("  {}  {s}", clock(*t));
    }

    rule("the disturbance boundary");
    let mut calm = Encounter::new(seed);
    calm.advance_to(end);
    let same = e.same_sectors(&calm, end);
    println!(
        "  {:<13} {:<22} {:>5} {:>10} {:>7} {:>9}  {}",
        "sector", "reached by", "woke", "stepped", "evals", "recompile", "against a world where nothing crossed"
    );
    for (i, s) in e.sectors.iter().enumerate() {
        let by: Vec<&str> = (0..SITES.len()).filter(|&k| field::reaches(k, i)).map(|k| SITES[k].name).collect();
        let verdict = if same[i] {
            "identical, bit for bit".to_string()
        } else {
            let ch = s.changed();
            format!("differs: {}", if ch.is_empty() { "lost output while down".into() } else { ch.join(", ") })
        };
        println!(
            "  {:<13} {:<22} {:>5} {:>10} {:>7} {:>9}  {}",
            s.spec.name,
            if by.is_empty() { "nothing".into() } else { by.join(", ") },
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
    println!("  {} bytes of log for {} derived events -- 0 serialized", log.len(), commas(f.stats.events as u128));
    let cks = e.checkpoints_taken;
    println!(
        "  {} checkpoints, {} bytes on average; the disturbance's part of the latest is {} bytes",
        cks,
        commas((e.checkpoint_bytes / cks.max(1)) as u128),
        e.fight.to_json().to_string().len()
    );

    rule("three reconstructions of the same tick");
    let live = e.hash();
    let replay = Encounter::replay(seed, &e.log, end).hash();
    println!("  live                          {live:016x}");
    println!("  replayed from the log         {replay:016x}");
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
        None => println!("  (no checkpoint fell inside the disturbance)"),
    }
    println!("  {}", if good { "all agree" } else { "THEY DISAGREE" });
    if good {
        0
    } else {
        1
    }
}

/// The lanes open for a minute and a quarter, and nothing else -- or, with
/// `anchored`, both anchors powered first so no bleed ever reaches the gantry.
fn volume(rate: u64, anchored: bool) -> Vec<(Tick, Cmd)> {
    let mut s: Vec<(Tick, Cmd)> =
        afternoon(rate).into_iter().filter(|(_, c)| matches!(c, Cmd::Stream { .. })).collect();
    if anchored {
        s.insert(0, (TICK_RATE, Cmd::Anchor { anchor: 1, on: true }));
        s.insert(0, (TICK_RATE, Cmd::Anchor { anchor: 0, on: true }));
    }
    s
}

fn scale(seed: u64) -> i32 {
    rule("unanchored: the district limits itself");
    println!("  {:>7} {:>12} {:>12} {:>10} {:>5}", "t/s", "asked t", "crossed t", "manifest", "tears");
    for rate in [10u64, 100, 1_000, 10_000] {
        let mut e = Encounter::new(seed);
        run_script(&mut e, &volume(rate, false), |_| {});
        settle(&mut e, 30 * 60 * TICK_RATE, |_| {});
        let f = &e.fight;
        let crossed: u64 = (0..LANES.len()).map(|l| f.crossed_at(l, e.now) / 1000).sum();
        let asked = (rate + (rate / 2).max(1)) * 78;
        println!(
            "  {:>7} {:>12} {:>12} {:>10} {:>5}",
            commas(rate as u128),
            commas(asked as u128),
            commas(crossed as u128),
            commas(f.spawned as u128),
            f.stats.tears
        );
    }
    println!("  A gantry that tears into 1890 goes dark, so pushing harder mostly buys more tears, not more tonnes.");

    rule("anchored: every tonne crosses, and the disturbance scales with it");
    println!(
        "  {:>7} {:>11} {:>11} {:>5} {:>7} {:>6} {:>7} {:>6} {:>11} {:>5} {:>13} {:>8} {:>8}",
        "t/s", "crossed t", "manifest", "peak", "records", "events", "visits", "killed", "faded", "tears", "lost", "runtime", "us/event"
    );
    let mut rows = Vec::new();
    for rate in [40u64, 400, 4_000, 40_000, 400_000] {
        let mut e = Encounter::new(seed);
        let t0 = Instant::now();
        run_script(&mut e, &volume(rate, true), |_| {});
        settle(&mut e, 30 * 60 * TICK_RATE, |_| {});
        let wall = t0.elapsed().as_secs_f64();
        let f = &e.fight;
        let crossed: u64 = (0..LANES.len()).map(|l| f.crossed_at(l, e.now) / 1000).sum();
        let lost = f.lost_structures();
        println!(
            "  {:>7} {:>11} {:>11} {:>5} {:>7} {:>6} {:>7} {:>6} {:>11} {:>5} {:>13} {:>6.1}ms {:>8.2}",
            commas(rate as u128),
            commas(crossed as u128),
            commas(f.spawned as u128),
            f.stats.peak_cohorts,
            f.stats.created,
            commas(f.stats.events as u128),
            commas(f.stats.visits as u128),
            commas(f.killed as u128),
            commas(f.faded as u128),
            f.stats.tears,
            if lost.is_empty() { "-".to_string() } else { lost.join(", ") },
            wall * 1000.0,
            wall * 1e6 / f.stats.events.max(1) as f64,
        );
        rows.push((f.spawned.max(1), f.stats.peak_cohorts, f.stats.events, wall));
    }
    let (first, last) = (rows[0], rows[rows.len() - 1]);
    println!();
    println!(
        "  {}x the manifestations, {:.1}x the peak cohorts, {:.1}x the events, {:.1}x the runtime.",
        commas((last.0 / first.0) as u128),
        last.1 as f64 / first.1.max(1) as f64,
        last.2 as f64 / first.2.max(1) as f64,
        last.3 / first.3.max(1e-9)
    );
    println!("  A cohort is one record whatever it stands for; a site is two numbers whatever crossed.");
    println!("  Past a few hundred thousand, the crowd razes the store it came for at once, and fades with it.");
    0
}
