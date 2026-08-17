//! Experiment harness: runs three factory configurations at increasing scale
//! and cross-validates every analysis tier against the event simulator.

use std::time::{Duration, Instant};
use temporal_rooms::analytic::{self, Rat};
use temporal_rooms::dsl;
use temporal_rooms::model::*;
use temporal_rooms::sim::{CountersBig, World};

const FAR: Tick = 1_000_000_000_000_000_000;

struct Cfg {
    title: String,
    path: String,
    /// Horizon for the materialised (T1) deployment run.
    t_mat: Tick,
    /// Cap on how many instances we are willing to materialise.
    max_inst: u64,
    /// Horizon answered analytically only.
    t_far: Tick,
    /// Extra single-instance state dumps.
    dumps: Vec<Tick>,
}

fn configs() -> Vec<Cfg> {
    vec![
        Cfg {
            title: "CONFIGURATION 1 -- the specification as written".into(),
            path: "configs/01-spec.factory".into(),
            t_mat: 5_000,
            max_inst: 1,
            t_far: FAR,
            dumps: vec![600, 1_200, 2_100, 5_000],
        },
        Cfg {
            title: "CONFIGURATION 2 -- sustainable line, 1,000,000 objects".into(),
            path: "configs/02-balanced.factory".into(),
            t_mat: 2_000,
            max_inst: 125_000,
            t_far: FAR,
            dumps: vec![600, 2_000],
        },
        Cfg {
            title: "CONFIGURATION 3 -- four stages, 1,000,000,005 objects".into(),
            path: "configs/03-megafactory.factory".into(),
            t_mat: 3_000,
            max_inst: 20_000,
            t_far: FAR,
            dumps: vec![3_000],
        },
    ]
}

/// `trooms <file.factory> ...` analyses just those files with default settings.
fn adhoc(path: &str) -> Cfg {
    Cfg {
        title: format!("{path}"),
        path: path.to_string(),
        t_mat: 3_000,
        max_inst: 20_000,
        t_far: FAR,
        dumps: vec![1_000, 3_000],
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfgs: Vec<Cfg> = if args.is_empty() {
        configs()
    } else {
        args.iter().map(|a| adhoc(a)).collect()
    };

    let mut failures = 0usize;
    let mut summary: Vec<String> = Vec::new();
    for cfg in &cfgs {
        match run(cfg) {
            Ok(line) => summary.push(line),
            Err(n) => {
                failures += n;
                summary.push(format!("{:<24} FAILED", cfg.path));
            }
        }
    }

    rule('=');
    println!("SUMMARY");
    rule('=');
    println!(
        "{:<14} {:>15} {:>13} {:>13} {:>11}",
        "config", "objects", "T1 sim", "T2+T4 exact", "speedup"
    );
    for s in &summary {
        println!("{s}");
    }
    println!();
    if failures == 0 {
        println!("all cross-validations passed: the closed form and the event\n\
                  simulator agree exactly, object for object.");
    } else {
        println!("{failures} cross-validation(s) FAILED");
        std::process::exit(1);
    }
}

fn run(cfg: &Cfg) -> Result<String, usize> {
    rule('=');
    println!("{}", cfg.title);
    rule('=');

    let src = match std::fs::read_to_string(&cfg.path) {
        Ok(s) => s,
        Err(e) => {
            println!("cannot read {}: {e}", cfg.path);
            return Err(1);
        }
    };
    let prog = match dsl::parse(&src) {
        Ok(p) => p,
        Err(e) => {
            println!("DSL error in {}: {e}", cfg.path);
            return Err(1);
        }
    };
    let d = prog.deploys[0];
    let bp = &prog.blueprints[d.blueprint as usize];
    let n_items = prog.items.len();
    let mut fails = 0usize;

    // ---------------------------------------------------------- plant
    println!("\n-- compiled plant ------------------------------------------");
    println!("blueprint          {}", bp.name);
    println!(
        "  machines         {}  ({})",
        bp.actors.len(),
        bp.actors
            .iter()
            .map(|a| format!("{} {}", a.name, a.kind.label()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "  storages         {}  ({})",
        bp.storages.len(),
        bp.storages
            .iter()
            .map(|s| format!("{} cap {}", s.name, s.capacity))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("  base period      {} ticks (lcm of all cycle times)", bp.base_period);
    println!("  objects/line     {}", bp.objects());
    println!("deployment         {} lines, stagger {}", commas(d.count as u128), d.stagger);
    println!("  TOTAL OBJECTS    {}", commas(prog.total_objects()));
    let arch = analytic::archetypes(bp, d.count, d.stagger);
    println!(
        "  phase archetypes {}  (this is what analysis cost scales with)",
        arch.len()
    );
    println!(
        "  materialised state would need {}",
        bytes(World::state_bytes(bp, d.count))
    );

    // ------------------------------------------------- T3 rate algebra
    println!("\n-- T3  rate algebra (no simulation at all) -----------------");
    let t3_start = Instant::now();
    let rr = analytic::rates(bp, n_items);
    let t3 = t3_start.elapsed();
    println!(
        "solved in {} ({} fixpoint iterations, converged: {})",
        dur(t3),
        rr.iterations,
        rr.converged
    );
    println!("  {:<14} {:>16} {:>9}", "machine", "cycles/tick", "duty");
    for (a, ad) in bp.actors.iter().enumerate() {
        let mark = if rr.bottlenecks.contains(&a) { "  <- at capacity" } else { "" };
        println!(
            "  {:<14} {:>16} {:>8.1}%{}",
            ad.name,
            rr.cycles[a].show(),
            rr.duty[a].to_f64() * 100.0,
            mark
        );
    }
    for &i in &rr.accumulators {
        println!(
            "  WARNING: {} is produced but never consumed -- with finite storage",
            prog.item_name(i)
        );
        println!("           it saturates and stalls everything upstream of it.");
    }
    if rr.terminal {
        println!("  VERDICT: terminal. Asymptotic throughput is zero; this plant stops.");
    } else {
        println!(
            "  VERDICT: sustainable. Steady output {}",
            (0..n_items)
                .filter(|&i| !rr.produced_per_tick[i].is_zero())
                .map(|i| format!("{} {}/tick", rr.produced_per_tick[i].show(), prog.item_name(i as ItemId)))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // ------------------------------------------------- T2 closed form
    println!("\n-- T2  periodic-orbit closed form --------------------------");
    let t2_start = Instant::now();
    let cf = analytic::orbit(bp, n_items, 20_000_000);
    let t2 = t2_start.elapsed();
    println!("solved in {}", dur(t2));
    println!("  {}", cf.describe());
    println!(
        "  transient: {} events, {} distinct states visited",
        commas(cf.transient_events as u128),
        commas(cf.states_visited as u128)
    );
    if !cf.found {
        println!("  cannot proceed without an orbit");
        return Err(1);
    }
    if !cf.frozen {
        println!("  per-orbit deltas (period {} ticks):", cf.period);
        for (i, name) in prog.items.iter().enumerate() {
            if cf.delta.produced[i] > 0 || cf.delta.consumed[i] > 0 {
                println!(
                    "    {:<10} +{} produced, -{} consumed  =>  {} /tick net produced",
                    name,
                    cf.delta.produced[i],
                    cf.delta.consumed[i],
                    cf.steady_output_per_tick(i).show()
                );
            }
        }
        println!("  measured duty per machine (exact, from the orbit):");
        for (a, ad) in bp.actors.iter().enumerate() {
            let exact = cf.steady_cycles_per_tick(a);
            let duty = exact.mul(Rat::new(ad.duration as u128, 1));
            let agree = exact == rr.cycles[a];
            println!(
                "    {:<14} {:>16}  duty {:>6.1}%   T3 says {:<16} {}",
                ad.name,
                exact.show(),
                duty.to_f64() * 100.0,
                rr.cycles[a].show(),
                if agree { "agree" } else { "DIVERGES" }
            );
        }
    }

    // --------------------------------- single-instance state snapshots
    println!("\n-- exact state of one line at selected ticks ---------------");
    for &t in &cfg.dumps {
        let (w, orbits) = cf.world_at(bp, n_items, t);
        let mut parts: Vec<String> = Vec::new();
        for (s, sd) in bp.storages.iter().enumerate() {
            let items: Vec<String> = sd
                .slots
                .iter()
                .filter(|&&it| w.storage_qty(0, s, it) > 0)
                .map(|&it| format!("{} {}", w.storage_qty(0, s, it), prog.item_name(it)))
                .collect();
            let body = if items.is_empty() { "empty".to_string() } else { items.join(" + ") };
            parts.push(format!("{}[{}/{}: {}]", sd.name, w.storage_used(0, s), sd.capacity, body));
        }
        let machines: Vec<String> = bp
            .actors
            .iter()
            .enumerate()
            .map(|(a, ad)| format!("{}={}", ad.name, World::state_name(w.actor_state(0, a))))
            .collect();
        println!("  t={:<8} {}", t, parts.join("  "));
        println!("           {}", machines.join(" "));
        if orbits > 0 {
            println!("           (reached by skipping {} whole orbits)", commas(orbits));
        }
    }

    // --------------------------- validation: T1 vs T2, single instance
    println!("\n-- validation: T1 event sim  vs  T2 closed form ------------");
    let probes: Vec<Tick> = {
        let mut v: Vec<Tick> = cfg.dumps.to_vec();
        v.extend([1, 59, 60, 61, 137, cfg.t_mat, cfg.t_mat + 1, cfg.t_mat * 3 + 7]);
        v.sort_unstable();
        v.dedup();
        v
    };
    let mut ok = true;
    for &t in &probes {
        let mut w = World::new(bp, n_items, 1, 0);
        w.run_until(t);
        let sim = CountersBig::from_narrow(&w.c);
        let ana = cf.eval(bp, n_items, t);
        if sim != ana {
            println!("  t={t}: MISMATCH");
            ok = false;
        }
    }
    println!(
        "  {} probe ticks up to t={}: {}",
        probes.len(),
        commas(*probes.last().unwrap() as u128),
        if ok { "identical" } else { "MISMATCH" }
    );
    if !ok {
        fails += 1;
    }

    // ------------------- validation: T1 whole deployment vs T4 archetypes
    println!("\n-- validation: T1 materialised deployment  vs  T4 -----------");
    let n_mat = d.count.min(cfg.max_inst);
    let objects_mat = n_mat as u128 * bp.objects() as u128;
    println!(
        "materialising {} lines = {} objects, running to t={}",
        commas(n_mat as u128),
        commas(objects_mat),
        cfg.t_mat
    );
    let build_start = Instant::now();
    let mut big = World::new(bp, n_items, n_mat, d.stagger);
    let build = build_start.elapsed();
    let bytes_used = big.total_bytes();
    println!(
        "  arena built in {}  ({}, {:.1} bytes/object)",
        dur(build),
        bytes(bytes_used),
        bytes_used as f64 / objects_mat as f64
    );

    let sim_start = Instant::now();
    big.run_until(cfg.t_mat);
    let sim_time = sim_start.elapsed();
    let eps = big.events as f64 / sim_time.as_secs_f64();
    println!(
        "  T1: {} events in {}  ({} events/s, {} events/object)",
        commas(big.events as u128),
        dur(sim_time),
        commas(eps as u128),
        big.events / objects_mat.max(1) as u64
    );

    let d_mat = Deploy { blueprint: d.blueprint, count: n_mat, stagger: d.stagger };
    let t4_start = Instant::now();
    let (totals, n_arch) = analytic::deployment_totals(bp, n_items, &cf, &d_mat, cfg.t_mat);
    let t4_time = t4_start.elapsed();
    println!(
        "  T4: same answer from {} archetype evaluations in {}",
        n_arch,
        dur(t4_time)
    );

    let sim_totals = CountersBig::from_narrow(&big.c);
    let agree = sim_totals == totals;
    println!(
        "  cross-check: {}",
        if agree { "EXACT MATCH on every counter" } else { "MISMATCH" }
    );
    if !agree {
        fails += 1;
        for (i, name) in prog.items.iter().enumerate() {
            println!(
                "    {:<10} sim produced {:>18}   analytic {:>18}",
                name,
                commas(sim_totals.produced[i]),
                commas(totals.produced[i])
            );
        }
    } else {
        for (i, name) in prog.items.iter().enumerate() {
            if totals.produced[i] > 0 {
                println!(
                    "    {:<10} produced {:>18}   consumed {:>18}",
                    name,
                    commas(totals.produced[i]),
                    commas(totals.consumed[i])
                );
            }
        }
    }
    let speedup = sim_time.as_secs_f64() / t4_time.as_secs_f64().max(1e-9);
    println!("  speedup at this scale and horizon: {:.0}x", speedup);

    // ------------------------------- full deployment at a far horizon
    println!("\n-- full deployment, analytically only ----------------------");
    let far_start = Instant::now();
    let (far, n_arch_full) = analytic::deployment_totals(bp, n_items, &cf, &d, cfg.t_far);
    let far_time = far_start.elapsed();
    println!(
        "{} objects at t={} ({} archetype evaluations) in {}",
        commas(prog.total_objects()),
        commas(cfg.t_far as u128),
        n_arch_full,
        dur(far_time)
    );
    for (i, name) in prog.items.iter().enumerate() {
        if far.produced[i] > 0 {
            println!("    {:<10} produced {:>30}", name, commas(far.produced[i]));
        }
    }

    // What T1 would have cost for the same question. A frozen plant empties its
    // event queue, so T1 terminates early too and there is nothing to save --
    // the closed form only buys time on plants that keep running.
    let scale_inst = d.count as f64 / n_mat as f64;
    let scale_time = cfg.t_far as f64 / cfg.t_mat as f64;
    let (est_events, est_secs) = if cf.frozen {
        (big.events as f64 * scale_inst, sim_time.as_secs_f64() * scale_inst)
    } else {
        (
            big.events as f64 * scale_inst * scale_time,
            sim_time.as_secs_f64() * scale_inst * scale_time,
        )
    };
    if cf.frozen {
        println!(
            "  this plant freezes, so T1 also terminates early: ~{:.2e} events (~{})",
            est_events,
            human_secs(est_secs)
        );
        println!("  no time-horizon speedup here -- only the {}x object-count saving.", commas(scale_inst as u128));
    } else {
        println!(
            "  the same question via T1 would need ~{:.2e} events (~{})",
            est_events,
            human_secs(est_secs)
        );
        println!(
            "  T2/T4 answered it in {} -- a factor of ~{:.1e}",
            dur(far_time),
            est_secs / far_time.as_secs_f64().max(1e-9)
        );
    }

    println!();
    let line = format!(
        "{:<14} {:>15} {:>13} {:>13} {:>11}",
        short(&cfg.path),
        commas(prog.total_objects()),
        human_secs(est_secs),
        dur(far_time),
        format!("{:.0e}x", est_secs / far_time.as_secs_f64().max(1e-9))
    );
    if fails > 0 {
        Err(fails)
    } else {
        Ok(line)
    }
}

// ------------------------------------------------------------ formatting

fn rule(c: char) {
    println!("{}", std::iter::repeat(c).take(64).collect::<String>());
}

fn short(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).replace(".factory", "")
}

fn commas(n: u128) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn bytes(n: u128) -> String {
    const U: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", U[i])
}

fn dur(d: Duration) -> String {
    let s = d.as_secs_f64();
    if s < 1e-3 {
        format!("{:.1} us", s * 1e6)
    } else if s < 1.0 {
        format!("{:.2} ms", s * 1e3)
    } else {
        format!("{s:.2} s")
    }
}

fn human_secs(s: f64) -> String {
    if s < 60.0 {
        format!("{s:.1} s")
    } else if s < 3600.0 {
        format!("{:.1} min", s / 60.0)
    } else if s < 86400.0 {
        format!("{:.1} hours", s / 3600.0)
    } else if s < 86400.0 * 365.25 {
        format!("{:.1} days", s / 86400.0)
    } else {
        format!("{:.2e} years", s / (86400.0 * 365.25))
    }
}
