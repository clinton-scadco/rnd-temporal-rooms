//! Experiment harness for v3.
//!
//! v1 asked whether a billion *independent* factory objects could be answered
//! without touching a billion objects. They could. v2 removed the independence
//! -- shared buffers, fan-in, fan-out, feedback cycles, batch transport -- and
//! got the same answer. v3 removes the single clock: a plant is cut at its
//! transports and the pieces are run as separate simulations at separate times.
//!
//! Every configuration is put through the same gauntlet, and every analytic
//! answer is checked against the event simulator that is not allowed to cheat.
//! The v3 answers are checked twice over: against the monolithic lumped solver
//! state for state, and against that same event simulator.

use std::time::{Duration, Instant};
use temporal_rooms::analytic::{self, Rat};
use temporal_rooms::domains;
use temporal_rooms::dsl;
use temporal_rooms::graph::Graph;
use temporal_rooms::live::{self, Command, Edit, Log};
use temporal_rooms::model::*;
use temporal_rooms::pop;
use temporal_rooms::rooms::{self, Room};
use temporal_rooms::scenario;
use temporal_rooms::sim::{self, CountersBig, World};
use temporal_rooms::web;

const FAR: Tick = 1_000_000_000_000_000_000;
/// Beyond this, materialising the machine list is not worth the RAM.
const MAT_MACHINE_CAP: u64 = 4_000_000;

struct Cfg {
    title: String,
    path: String,
    /// Horizon for the materialised (T1) run.
    t_mat: Tick,
    /// Cap on how many blueprint instances we are willing to materialise.
    max_inst: u64,
    /// Horizon answered analytically only.
    t_far: Tick,
    /// Extra single-instance state dumps.
    dumps: Vec<Tick>,
}

fn cfg(title: &str, path: &str, t_mat: Tick, max_inst: u64, dumps: &[Tick]) -> Cfg {
    Cfg {
        title: title.into(),
        path: path.into(),
        t_mat,
        max_inst,
        t_far: FAR,
        dumps: dumps.to_vec(),
    }
}

fn configs() -> Vec<Cfg> {
    vec![
        cfg("CONFIG 1 -- v1's reference plant, still deadlocking", "configs/01-spec.factory", 5_000, 1, &[600, 2_100]),
        cfg("CONFIG 2 -- v1's balanced line, 1,000,000 objects", "configs/02-balanced.factory", 2_000, 125_000, &[600, 2_000]),
        cfg("CONFIG 3 -- v1's four-stage chain, 1,000,000,005 objects", "configs/03-megafactory.factory", 3_000, 20_000, &[3_000]),
        cfg("CONFIG 4 -- two-ingredient recipe", "configs/04-science.factory", 3_000, 2_000, &[3_000]),
        cfg("CONFIG 5 -- fan-in, fan-out, and rude periods", "configs/05-coupled.factory", 6_000, 1, &[600, 6_000]),
        cfg("CONFIG 6 -- a feedback cycle with a catalyst", "configs/06-cycle.factory", 6_000, 1, &[600, 6_000]),
        cfg("CONFIG 7 -- two domains joined by a slow train", "configs/07-transport.factory", 40_000, 1, &[3_000, 40_000]),
        cfg("CONFIG 8 -- contention policies, side by side", "configs/08-policy.factory", 6_000, 1, &[6_000]),
        cfg("CONFIG 9 -- 10,000 smelters on one ore bay", "configs/09-population.factory", 4_000, 1, &[4_000]),
        cfg("CONFIG 10 -- one billion coupled machines", "configs/10-billion.factory", 4_000, 1, &[4_000]),
        cfg("CONFIG 11 -- three regions on a rail line", "configs/11-railchain.factory", 40_000, 1, &[8_000, 40_000]),
        cfg("CONFIG 12 -- two regions that trade both ways", "configs/12-tradeloop.factory", 30_000, 1, &[6_000, 30_000]),
        cfg("CONFIG 13 -- 250,000,000 lines on one ore field", "configs/13-orefield.factory", 4_000, 1, &[4_000]),
        cfg("CONFIG 14 -- the same field, private bays", "configs/14-privatebay.factory", 20_000, 1, &[20_000]),
        cfg("CONFIG 15 -- 1.5 billion machines in six regions", "configs/15-continent.factory", 20_000, 1, &[20_000]),
    ]
}

fn adhoc(path: &str) -> Cfg {
    cfg(path, path, 3_000, 2_000, &[1_000, 3_000])
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Two subcommands sit beside the harness. `serve` is the workbench; the
    // harness prints tables, and a table is a poor way to look at a factory.
    match args.first().map(String::as_str) {
        Some("serve") => {
            let port = args
                .iter()
                .position(|a| a == "--port")
                .and_then(|i| args.get(i + 1))
                .and_then(|p| p.parse().ok())
                .unwrap_or(8787);
            if let Err(e) = web::serve(port) {
                eprintln!("cannot serve on port {port}: {e}");
                std::process::exit(1);
            }
            return;
        }
        // A self-contained deterministic trace: the same snapshots the
        // workbench asks for, taken at a schedule of ticks and written out, so
        // a viewer with no simulator can still render the plant at any of them.
        Some("export") => {
            let Some(path) = args.get(1) else {
                eprintln!("usage: trooms export <config.factory> [--out FILE] [ticks...]");
                std::process::exit(1);
            };
            let out = args
                .iter()
                .position(|a| a == "--out")
                .and_then(|i| args.get(i + 1))
                .cloned()
                .unwrap_or_else(|| "trace.json".to_string());
            let mut ticks: Vec<Tick> =
                args[2..].iter().filter_map(|a| a.parse().ok()).collect();
            if ticks.is_empty() {
                ticks = (0..=200).map(|k| k * 200).collect();
            }
            match web::export(path, &ticks) {
                Ok(doc) => {
                    if let Err(e) = std::fs::write(&out, doc.as_bytes()) {
                        eprintln!("cannot write {out}: {e}");
                        std::process::exit(1);
                    }
                    let bytes = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                    println!("{} frames of {path} -> {out} ({} bytes)", ticks.len(), commas(bytes as u128));
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
            return;
        }
        // A scenario, played headlessly. The workbench is a nicer way to look
        // at this, and a scoreboard that only exists inside a browser is a
        // scoreboard nobody can put in a test or a shell pipe.
        Some("play") => {
            let Some(path) = args.get(1) else {
                eprintln!("usage: trooms play <scenario.scenario> [--buy Name=N@Tick]... [--at TICK]");
                std::process::exit(1);
            };
            if let Err(e) = play(path, &args[2..]) {
                eprintln!("{e}");
                std::process::exit(1);
            }
            return;
        }
        _ => {}
    }

    let cfgs: Vec<Cfg> =
        if args.is_empty() { configs() } else { args.iter().map(|a| adhoc(a)).collect() };

    let mut failures = 0usize;
    let mut summary: Vec<String> = Vec::new();
    for c in &cfgs {
        match run(c) {
            Ok(line) => summary.push(line),
            Err(n) => {
                failures += n;
                summary.push(format!("{:<16} {:>15} FAILED", short(&c.path), ""));
            }
        }
    }

    rule('=');
    println!("SUMMARY");
    rule('=');
    println!(
        "{:<16} {:>15} {:>8} {:>10} {:>8} {:>10} {:>10} {:>11}",
        "config", "objects", "classes", "pop cells", "regions", "drift", "T5 solve", "compression"
    );
    for s in &summary {
        println!("{s}");
    }
    println!();
    if failures == 0 {
        println!(
            "all cross-validations passed. The lumped population solver and the\n\
             machine-by-machine event simulator agree exactly, on coupled plants."
        );
    } else {
        println!("{failures} cross-validation(s) FAILED");
        std::process::exit(1);
    }
}

fn run(c: &Cfg) -> Result<String, usize> {
    rule('=');
    println!("{}", c.title);
    rule('=');

    let src = match std::fs::read_to_string(&c.path) {
        Ok(s) => s,
        Err(e) => {
            println!("cannot read {}: {e}", c.path);
            return Err(1);
        }
    };
    let prog = match dsl::parse(&src) {
        Ok(p) => p,
        Err(e) => {
            println!("DSL error in {}: {e}", c.path);
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
    println!("  classes          {}", bp.actors.len());
    for a in &bp.actors {
        println!(
            "    {:<14} {:<10} x{:<13} cycle {:>6} ticks",
            a.name,
            a.kind.label(),
            commas(a.count as u128),
            a.duration
        );
    }
    println!("  storages         {}", bp.storages.len());
    for s in &bp.storages {
        let init: String = if s.initial.is_empty() {
            String::new()
        } else {
            format!(
                "  initial {}",
                s.initial
                    .iter()
                    .map(|st| format!("{} {}", st.qty, prog.item_name(st.item)))
                    .collect::<Vec<_>>()
                    .join(" + ")
            )
        };
        println!(
            "    {:<14} cap {:<10} policy {:<12}{}",
            s.name,
            commas(s.capacity as u128),
            s.policy.label(),
            init
        );
    }
    println!("  machines/line    {}", commas(bp.machines as u128));
    println!("  base period      {} ticks (lcm of all cycle times)", bp.base_period);
    println!("deployment         {} lines, stagger {}", commas(d.count as u128), d.stagger);
    println!("  TOTAL OBJECTS    {}", commas(prog.total_objects()));

    // ------------------------------------------------------- coupling
    println!("\n-- coupling and causal domains -----------------------------");
    let rep = domains::analyse(bp);
    if rep.withdraw_contention.is_empty() && rep.deposit_contention.is_empty() {
        println!("no contention: every storage has one taker and one giver.");
    }
    for (s, cs, n) in &rep.withdraw_contention {
        println!(
            "  {} is drawn from by {} machines in {} class(es) [{}] -- policy {}",
            bp.storages[*s as usize].name,
            commas(*n as u128),
            cs.len(),
            cs.iter().map(|&i| bp.actors[i as usize].name.as_str()).collect::<Vec<_>>().join(", "),
            bp.storages[*s as usize].policy.label()
        );
    }
    for (s, cs, n) in &rep.deposit_contention {
        println!(
            "  {} is filled by {} machines in {} class(es) [{}]",
            bp.storages[*s as usize].name,
            commas(*n as u128),
            cs.len(),
            cs.iter().map(|&i| bp.actors[i as usize].name.as_str()).collect::<Vec<_>>().join(", ")
        );
    }
    if !rep.feedback_classes.is_empty() {
        println!(
            "  feedback cycle through: {}",
            rep.feedback_classes
                .iter()
                .map(|&i| bp.actors[i as usize].name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    println!("  hard domains     {} (parts that never interact at all)", rep.hard.len());
    println!("  transit domains  {} (parts that interact only through transport)", rep.transit.len());
    if rep.transit.len() > rep.hard.len() {
        for (i, dom) in rep.transit.iter().enumerate() {
            let names: Vec<String> =
                dom.nodes.iter().map(|&n| domains::node_name(bp, n)).collect();
            println!("    domain {i}: {}", names.join(" "));
            match dom.independent_for() {
                Some(w) => println!(
                    "      {} machines, {} buffer; can be advanced alone for {} ticks",
                    commas(dom.machines as u128),
                    commas(dom.capacity as u128),
                    commas(w as u128)
                ),
                None => println!(
                    "      {} machines, {} buffer; nothing ever arrives, independent forever",
                    commas(dom.machines as u128),
                    commas(dom.capacity as u128)
                ),
            }
        }
    }

    // ------------------------------- v3 deployments that share a network
    if let Some(org) = d.origin {
        println!("\n-- v3  a deployment whose lines share infrastructure -------");
        let ob = &prog.blueprints[org.blueprint as usize];
        let shared: Vec<&str> = ob
            .storages
            .iter()
            .filter(|s| s.shared)
            .map(|s| s.name.as_str())
            .collect();
        let private: Vec<&str> = ob
            .storages
            .iter()
            .filter(|s| !s.shared)
            .map(|s| s.name.as_str())
            .collect();
        println!(
            "  {} lines share [{}]{}",
            commas(org.lines as u128),
            shared.join(", "),
            if private.is_empty() {
                " and keep nothing private".to_string()
            } else {
                format!(" and keep [{}] private", private.join(", "))
            }
        );
        if org.collapsed {
            println!(
                "  Nothing is private, so no state distinguishes one line from another\n\
                 \x20 and the whole deployment is one population:"
            );
            for a in &bp.actors {
                println!(
                    "    {:<12} x{:<16}{}",
                    a.name,
                    commas(a.count as u128),
                    if a.shared { "(shared: one set for everybody)" } else { "" }
                );
            }
            println!("  the claim, checked against the plant written out line by line:");
            println!(
                "    {:>6} {:>9} {:>10} {:>12}  {}",
                "lines", "classes", "machines", "probe ticks", "agreement"
            );
            let mut all_ok = true;
            for n in [1u64, 2, 3, 5, 8, 13] {
                let wide = ob.spread(n);
                let tall = ob.collapse(n);
                let probes: Vec<Tick> = vec![1, 20, 41, 137, 400, 1_000, 2_000, 5_000];
                let mut ok = true;
                for &t in &probes {
                    let mut a = pop::Pop::new(&wide, n_items);
                    a.run_until(t);
                    let mut b = pop::Pop::new(&tall, n_items);
                    b.run_until(t);
                    if a.c.produced != b.c.produced || a.c.consumed != b.c.consumed {
                        ok = false;
                    }
                    // Cycles are per class, and the wide plant has `n` classes
                    // where the tall one has a single populous one, so they are
                    // summed back before comparing.
                    let mut k = 0usize;
                    for (i, orig) in ob.actors.iter().enumerate() {
                        let reps = if orig.shared { 1 } else { n as usize };
                        let sum: u64 = (0..reps).map(|j| a.c.cycles[k + j]).sum();
                        if sum != b.c.cycles[i] {
                            ok = false;
                        }
                        k += reps;
                    }
                }
                all_ok &= ok;
                println!(
                    "    {:>6} {:>9} {:>10} {:>12}  {}",
                    n,
                    wide.actors.len(),
                    commas(wide.machines as u128),
                    probes.len(),
                    if ok { "exact" } else { "MISMATCH" }
                );
            }
            if !all_ok {
                fails += 1;
            }
            println!(
                "  Two lines drawing on one bay are not independent for a single tick.\n\
                 \x20 They are still interchangeable, and that is the property the\n\
                 \x20 compression actually needed -- v1 asked for the stronger one."
            );
        } else {
            println!(
                "  A private bay is state that tells one line from another, so the\n\
                 \x20 lines are not interchangeable and the deployment cannot become a\n\
                 \x20 population. It is written out line by line instead: {} classes,\n\
                 \x20 {} storages, {} machines.",
                bp.actors.len(),
                bp.storages.len(),
                commas(bp.machines as u128)
            );
            println!("  how different do the lines actually get?");
            println!(
                "    {:>9} {:>16} {:>18}",
                "tick", "distinct lines", "distinct bay levels"
            );
            let mut p = pop::Pop::new(bp, n_items);
            for &t in &[200u64, 1_000, 5_000, 20_000, 60_000, 200_000] {
                p.run_until(t);
                let (lines, levels) = line_states(bp, &p, org.lines as usize);
                println!("    {:>9} {:>16} {:>18}", commas(t as u128), lines, levels);
            }
            println!(
                "  Sixteen lines, and the state space they occupy stays a handful wide.\n\
                 \x20 That is the v4 question in one table: a deployment may yet be a\n\
                 \x20 population of *line* states rather than of machine states."
            );
        }
    }

    // ---------------------------------------------- v3 room execution
    println!("\n-- v3  the Room: regions advancing on their own clocks -----");
    let plan = rooms::plan(bp);
    let g = &plan.graph;
    print!("  regions          {}", g.regions.len());
    if g.fused > 0 {
        print!(
            "   ({} transit domain(s) glued back on: a link whose vehicle\n\
             \x20                      teleports home pins its two ends into lockstep)",
            g.fused
        );
    }
    println!();
    for (i, reg) in g.regions.iter().enumerate() {
        let names: Vec<String> = reg
            .storages
            .iter()
            .map(|&s| bp.storages[s as usize].name.clone())
            .chain(reg.classes.iter().map(|&c| {
                let a = &bp.actors[c as usize];
                if a.count == 1 {
                    a.name.clone()
                } else {
                    format!("{}x{}", a.name, a.count)
                }
            }))
            .collect();
        println!(
            "    region {i}: {}{}",
            names.iter().take(8).cloned().collect::<Vec<_>>().join(" "),
            if names.len() > 8 {
                format!(" ... and {} more", names.len() - 8)
            } else {
                String::new()
            }
        );
        println!(
            "      {} machines, {} buffer, guaranteed slack {}",
            commas(reg.machines as u128),
            commas(reg.capacity as u128),
            match reg.slack(&g.channels) {
                Some(s) => format!("{} ticks", commas(s as u128)),
                None => "unbounded -- it hears from nobody".to_string(),
            }
        );
    }
    for ch in &g.channels {
        let a = &bp.actors[ch.class as usize];
        let (num, den) = a.throughput();
        println!(
            "    channel {}: region {} -> region {}",
            a.name, ch.src_region, ch.dst_region
        );
        println!(
            "      {} vehicles x {} {},  {} ticks out / {} ticks home  =  {} items/tick",
            commas(a.count as u128),
            commas(a.inputs[0].qty as u128),
            prog.item_name(a.inputs[0].item),
            commas(ch.latency as u128),
            commas(ch.return_latency as u128),
            Rat::new(num, den).show()
        );
        if let Some(geo) = a.geometry {
            println!(
                "      latency derived from geometry: {} + {}/{} = {}",
                geo.base, geo.distance, geo.speed, ch.latency
            );
        }
    }

    // Run the decomposition and check it against the monolithic solver, state
    // for state rather than merely counter for counter.
    let room_probes: Vec<Tick> = {
        let mut v: Vec<Tick> = c.dumps.clone();
        v.extend([1, 17, 137, 999, 1_500, 3_000, c.t_mat / 2, c.t_mat, c.t_mat + 1]);
        v.retain(|&t| t > 0);
        v.sort_unstable();
        v.dedup();
        v
    };
    let mut room_ok = true;
    for &t in &room_probes {
        let mut room = Room::new(&plan, n_items);
        room.run_until(t);
        let mut mono = pop::Pop::new(bp, n_items);
        mono.run_until(t);
        if room.signature(bp) != mono.signature() || room.counters() != mono.c {
            println!("  t={t}: MISMATCH between the decomposed and monolithic states");
            room_ok = false;
        }
    }
    println!(
        "  {} probe ticks: decomposed state {} monolithic state",
        room_probes.len(),
        if room_ok { "== byte for byte ==" } else { "DIFFERS FROM" }
    );
    if !room_ok {
        fails += 1;
    }

    let ts = Instant::now();
    let mut room = Room::new(&plan, n_items);
    room.run_until(c.t_mat);
    let room_time = ts.elapsed();
    let ts = Instant::now();
    let mut mono = pop::Pop::new(bp, n_items);
    mono.run_until(c.t_mat);
    let mono_time = ts.elapsed();
    println!(
        "  to t={}: {} region advances, {} messages, {} rendezvous",
        commas(c.t_mat as u128),
        commas(room.steps as u128),
        commas(room.messages as u128),
        commas(room.rendezvous as u128)
    );
    println!(
        "    a region ran alone for {:.0} ticks on average, {} at most",
        room.mean_advance(),
        commas(room.max_advance as u128)
    );
    for (i, m) in room.modes.iter().enumerate() {
        println!("    region {i} solved as: {}", m.label());
    }
    if g.regions.len() > 1 {
        println!(
            "    widest clock skew {} ticks, at {}",
            commas(room.max_skew as u128),
            room.skew_clocks
                .iter()
                .enumerate()
                .map(|(i, t)| format!("r{i} t={}", commas(*t as u128)))
                .collect::<Vec<_>>()
                .join("  ")
        );
    }
    println!(
        "    decomposed {} vs monolithic {}",
        dur(room_time),
        dur(mono_time)
    );

    // The same plant with v2's teleporting vehicles, to show what the trip
    // home is actually buying.
    if !bp.links().is_empty() {
        let flat = with_teleporting_links(bp);
        let fg = domains::regions(&flat);
        println!(
            "  delete every trip home (v2's link) and the same plant has {} region(s)\n\
             \x20   instead of {}: {}",
            fg.regions.len(),
            g.regions.len(),
            if fg.fused > 0 {
                format!(
                    "{} of them are pinned into lockstep by a zero-cost\n\
                     \x20   channel running backwards through the transport",
                    fg.fused + 1
                )
            } else {
                format!(
                    "still {}, but the sending side's slack falls to {}",
                    fg.regions.len(),
                    fg.min_slack()
                        .map(|s| format!("{s} ticks"))
                        .unwrap_or_else(|| "unbounded".into())
                )
            }
        );
    }

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
    println!("  {:<14} {:>18} {:>12}", "class", "cycles/tick", "duty/machine");
    for (a, ad) in bp.actors.iter().enumerate() {
        let mark = if rr.bottlenecks.contains(&a) { "  <- at capacity" } else { "" };
        println!(
            "  {:<14} {:>18} {:>11.1}%{}",
            ad.name,
            rr.cycles[a].show(),
            rr.duty[a].to_f64() * 100.0,
            mark
        );
    }
    for &i in &rr.unattainable {
        println!(
            "  DEAD: {} can never exist -- making it requires already having it,",
            prog.item_name(i)
        );
        println!("        and no storage seeds any. Add an `initial` to break the cycle.");
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
                .map(|i| format!(
                    "{} {}/tick",
                    rr.produced_per_tick[i].show(),
                    prog.item_name(i as ItemId)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // ------------------------------------------- T5 population solver
    println!("\n-- T5  lumped population closed form -----------------------");
    let t5_start = Instant::now();
    let pf = pop::orbit(bp, n_items, 20_000_000);
    let t5_time = t5_start.elapsed();
    println!("solved in {}", dur(t5_time));
    println!("  {}", pf.describe());
    println!(
        "  transient: {} batch grants over {} rounds, {} distinct population states",
        commas(pf.grants as u128),
        commas(pf.rounds as u128),
        commas(pf.states_visited as u128)
    );
    println!(
        "  COMPRESSION: {} machines held in at most {} occupied cells  ({:.0}x)",
        commas(pf.population as u128),
        pf.max_distinct_states,
        pf.population as f64 / pf.max_distinct_states.max(1) as f64
    );
    if !pf.found {
        println!("  cannot proceed without an orbit");
        return Err(1);
    }
    if !pf.frozen {
        println!("  per-orbit deltas (period {} ticks):", pf.period);
        for (i, name) in prog.items.iter().enumerate() {
            if pf.delta.produced[i] > 0 || pf.delta.consumed[i] > 0 {
                println!(
                    "    {:<12} +{} produced, -{} consumed  =>  {} /tick net",
                    name,
                    commas(pf.delta.produced[i] as u128),
                    commas(pf.delta.consumed[i] as u128),
                    Rat::new(
                        (pf.delta.produced[i] - pf.delta.consumed[i].min(pf.delta.produced[i]))
                            as u128,
                        pf.period as u128
                    )
                    .show()
                );
            }
        }
        println!("  exact duty per machine, and what the fluid model guessed:");
        let mut diverged = false;
        for (a, ad) in bp.actors.iter().enumerate() {
            let cyc = Rat::new(pf.delta.cycles[a] as u128, pf.period as u128);
            let duty = cyc.mul(Rat::new(ad.cycle() as u128, 1)).div(Rat::new(ad.count as u128, 1));
            let agree = cyc == rr.cycles[a];
            println!(
                "    {:<14} {:>16}  duty {:>6.1}%   T3 said {:<16} {}",
                ad.name,
                cyc.show(),
                duty.to_f64() * 100.0,
                rr.cycles[a].show(),
                if agree { "agree" } else { "DIVERGES" }
            );
            diverged |= !agree;
        }
        if diverged {
            println!(
                "  Where they disagree, T3 is not merely imprecise. A fluid model has
                   to assume *some* sharing rule to divide a scarce input, and it
                   assumes each machine takes a share proportional to its appetite.
                   That is a contention policy -- an unstated one, and not the one
                   this plant declared. Aggregate throughput still comes out right;
                   who did the work does not."
            );
        }
    }

    // ------------------------------- population state at chosen ticks
    println!("\n-- population state at selected ticks ----------------------");
    for &t in &c.dumps {
        let (p, orbits) = pf.state_at(bp, n_items, t);
        let stores: Vec<String> = bp
            .storages
            .iter()
            .enumerate()
            .map(|(s, sd)| {
                let items: Vec<String> = sd
                    .slots
                    .iter()
                    .filter(|&&it| p.storage_qty(s, it) > 0)
                    .map(|&it| format!("{} {}", p.storage_qty(s, it), prog.item_name(it)))
                    .collect();
                let body = if items.is_empty() { "empty".into() } else { items.join(" + ") };
                format!("{}[{}/{}: {}]", sd.name, p.storage_used(s), sd.capacity, body)
            })
            .collect();
        println!("  t={:<8} {}", t, stores.join("  "));
        for a in 0..bp.actors.len() {
            println!("           {}", p.describe_class(a));
        }
        if orbits > 0 {
            println!("           (reached by skipping {} whole orbits)", commas(orbits));
        }
    }

    // ------------------------------------------ THE cross-validation
    println!("\n-- validation: T1 machine-by-machine  vs  T5 lumped ---------");
    let can_materialise = bp.machines <= MAT_MACHINE_CAP;
    if !can_materialise {
        println!(
            "  {} machines per line is past the {} materialisation cap:",
            commas(bp.machines as u128),
            commas(MAT_MACHINE_CAP as u128)
        );
        println!("  T1 cannot run here at all. That is the point of the tier.");
    } else {
        let probes: Vec<Tick> = {
            let mut v: Vec<Tick> = c.dumps.clone();
            v.extend([1, 17, 59, 60, 61, 137, 500, c.t_mat, c.t_mat + 1]);
            v.sort_unstable();
            v.dedup();
            v
        };
        let mut ok = true;
        for &t in &probes {
            let mut w = World::new(bp, n_items, 1, 0);
            w.run_until(t);
            let a = CountersBig::from_narrow(&w.c);
            let b = pf.eval(bp, n_items, t);
            if a != b {
                println!("  t={t}: MISMATCH");
                for (i, name) in prog.items.iter().enumerate() {
                    if a.produced[i] != b.produced[i] || a.consumed[i] != b.consumed[i] {
                        println!(
                            "    {name:<12} T1 +{} -{}   T5 +{} -{}",
                            a.produced[i], a.consumed[i], b.produced[i], b.consumed[i]
                        );
                    }
                }
                ok = false;
            }
        }
        println!(
            "  {} probe ticks up to t={}: {}",
            probes.len(),
            commas(*probes.last().unwrap() as u128),
            if ok { "identical on every counter" } else { "MISMATCH" }
        );
        if !ok {
            fails += 1;
        }

        // Cost comparison at one horizon.
        let mut w = World::new(bp, n_items, 1, 0);
        let ts = Instant::now();
        w.run_until(c.t_mat);
        let t1_time = ts.elapsed();
        let ts = Instant::now();
        let _ = pf.eval(bp, n_items, c.t_mat);
        let t5_eval = ts.elapsed();
        println!(
            "  to t={}: T1 needed {} events / {} rounds in {}; T5 answered in {}",
            commas(c.t_mat as u128),
            commas(w.events as u128),
            commas(w.rounds as u128),
            dur(t1_time),
            dur(t5_eval)
        );
    }

    // ---------------------------------------------- fairness histogram
    if can_materialise && bp.machines <= 200_000 && bp.actors.iter().any(|a| a.count > 1) {
        println!("\n-- who actually did the work (per-machine, t={}) ----------", c.t_mat);
        let mut w = World::new_tracked(bp, n_items, 1, 0);
        w.run_until(c.t_mat);
        for ad in bp.actors.iter() {
            if ad.count == 1 {
                continue;
            }
            let mut counts: Vec<u32> = (ad.machine_offset..ad.machine_offset + ad.count)
                .map(|m| w.member_cycles(0, m))
                .collect();
            counts.sort_unstable();
            let lo = *counts.first().unwrap();
            let hi = *counts.last().unwrap();
            println!(
                "  {:<14} x{:<9} cycles per machine: min {:<6} max {:<6} gap {}{}",
                ad.name,
                commas(ad.count as u128),
                lo,
                hi,
                hi - lo,
                if hi - lo <= 1 { "   (perfectly shared)" } else { "" }
            );
        }
        println!(
            "  Cycles are whole, so a gap of one is as even as a split can be. v1\n\
             \x20 could not reach it at all: with the lower array index always winning,\n\
             \x20 the gap between the luckiest and unluckiest machine grew without bound."
        );
    }

    // ----------------------------------------- policy is a design choice
    if rep.withdraw_contention.iter().any(|(_, cs, _)| cs.len() > 1) {
        println!("\n-- the same plant, resolved under each contention policy ----");
        println!("  storage capacity, recipes and periods are untouched; only the");
        println!("  rule for who gets served first changes.");
        let pols = [Policy::Index, Policy::RoundRobin];
        let mut cols: Vec<Vec<String>> = Vec::new();
        for pol in pols {
            let b2 = with_policy(bp, pol);
            let f = pop::orbit(&b2, n_items, 20_000_000);
            cols.push(
                (0..b2.actors.len())
                    .map(|a| {
                        if !f.found || f.period == 0 {
                            "stalled".to_string()
                        } else {
                            let cyc = Rat::new(f.delta.cycles[a] as u128, f.period as u128);
                            let duty = cyc
                                .mul(Rat::new(b2.actors[a].cycle() as u128, 1))
                                .div(Rat::new(b2.actors[a].count as u128, 1));
                            format!("{:.1}%", duty.to_f64() * 100.0)
                        }
                    })
                    .collect(),
            );
        }
        println!("  {:<14} {:>12} {:>14}", "class", "index", "round_robin");
        for (a, ad) in bp.actors.iter().enumerate() {
            let differ = cols[0][a] != cols[1][a];
            println!(
                "  {:<14} {:>12} {:>14}{}",
                ad.name,
                cols[0][a],
                cols[1][a],
                if differ { "   <- the policy decided this" } else { "" }
            );
        }
    }

    // --------------------------- T2/T4: the v1 tiers, where still viable
    println!("\n-- T2 + T4  (v1's tiers) on this plant ---------------------");
    let arch = analytic::archetypes(bp, d.count, d.stagger);
    if !can_materialise {
        println!("  T2 walks a materialised machine list, so it stops here too.");
        println!("  T5 covers the same ground without one.");
    } else {
        let ts = Instant::now();
        let cf = analytic::orbit(bp, n_items, 20_000_000);
        let t2_time = ts.elapsed();
        println!("  T2 {} (in {})", cf.describe(), dur(t2_time));
        if cf.found && !cf.frozen && !pf.frozen {
            let same = cf.period == pf.period && cf.delta == pf.delta;
            println!(
                "  T2 period {} vs T5 period {}: {}",
                cf.period,
                pf.period,
                if same { "identical orbit" } else { "differ (see note below)" }
            );
            if !same {
                println!(
                    "     T5 quotients out machine identity, so its orbit can close\n\
                     \x20    earlier or later; only the counters have to agree."
                );
            }
        }
        println!("  T4 phase archetypes for {} lines: {}", commas(d.count as u128), arch.len());

        // Materialised deployment vs archetype closed form.
        let n_mat = d.count.min(c.max_inst).min(MAT_MACHINE_CAP / bp.machines.max(1)).max(1);
        let objects = n_mat as u128 * bp.objects() as u128;
        let mut big = World::new(bp, n_items, n_mat, d.stagger);
        let bytes = big.total_bytes();
        let ts = Instant::now();
        big.run_until(c.t_mat);
        let sim_time = ts.elapsed();
        println!(
            "  materialised {} lines = {} objects ({}, {:.1} B/object)",
            commas(n_mat as u128),
            commas(objects),
            bytes_h(bytes),
            bytes as f64 / objects as f64
        );
        println!(
            "    T1: {} events in {} ({} events/s)",
            commas(big.events as u128),
            dur(sim_time),
            commas((big.events as f64 / sim_time.as_secs_f64().max(1e-12)) as u128)
        );
        let dm = Deploy { count: n_mat, ..d };
        let ts = Instant::now();
        let (totals, n_arch) = pop::deployment_totals(bp, n_items, &pf, &dm, c.t_mat);
        let t45 = ts.elapsed();
        let agree = CountersBig::from_narrow(&big.c) == totals;
        println!(
            "    T4+T5: same answer from {} archetype evaluations in {} -- {}",
            n_arch,
            dur(t45),
            if agree { "EXACT MATCH" } else { "MISMATCH" }
        );
        if !agree {
            fails += 1;
            for (i, name) in prog.items.iter().enumerate() {
                println!(
                    "      {name:<12} T1 {:>20}   T4+T5 {:>20}",
                    commas(big.c.produced[i] as u128),
                    commas(totals.produced[i])
                );
            }
        }
    }

    // ------------------------------- full deployment at a far horizon
    println!("\n-- the whole thing at t = 10^18 ----------------------------");
    let ts = Instant::now();
    let (far, n_arch) = pop::deployment_totals(bp, n_items, &pf, &d, c.t_far);
    let far_time = ts.elapsed();
    println!(
        "{} objects, {} archetype evaluations, {}",
        commas(prog.total_objects()),
        n_arch,
        dur(far_time)
    );
    for (i, name) in prog.items.iter().enumerate() {
        if far.produced[i] > 0 {
            println!("    {:<12} produced {:>34}", name, commas(far.produced[i]));
        }
    }

    println!();
    let line = format!(
        "{:<16} {:>15} {:>8} {:>10} {:>8} {:>10} {:>10} {:>11}",
        short(&c.path),
        commas(prog.total_objects()),
        bp.actors.len(),
        pf.max_distinct_states,
        g.regions.len(),
        commas(room.max_skew as u128),
        dur(t5_time),
        format!("{:.0}x", pf.population as f64 / pf.max_distinct_states.max(1) as f64)
    );
    if fails > 0 {
        Err(fails)
    } else {
        Ok(line)
    }
}


/// How many genuinely distinct states the lines of a spread deployment are in,
/// and how many distinct levels their private bays are at.
///
/// The lines of a spread plant are named `Thing#k`, so the suffix is the line
/// number. Shared nodes have no suffix and belong to nobody.
fn line_states(bp: &Blueprint, p: &pop::Pop, lines: usize) -> (usize, usize) {
    let mut sigs: Vec<Vec<u8>> = vec![Vec::new(); lines];
    let mut levels: Vec<Qty> = vec![0; lines];
    for (s, sd) in bp.storages.iter().enumerate() {
        let Some(k) = suffix_index(&sd.name).filter(|&k| k < lines) else { continue };
        levels[k] = p.storage_used(s);
        sigs[k].extend_from_slice(&p.storage_used(s).to_le_bytes());
        for &it in &sd.slots {
            sigs[k].extend_from_slice(&p.storage_qty(s, it).to_le_bytes());
        }
    }
    for (c, ad) in bp.actors.iter().enumerate() {
        let Some(k) = suffix_index(&ad.name).filter(|&k| k < lines) else { continue };
        let cp = &p.classes[c];
        sigs[k].extend_from_slice(&cp.starved.to_le_bytes());
        sigs[k].extend_from_slice(&cp.done.to_le_bytes());
        for (dl, n) in &cp.working {
            sigs[k].extend_from_slice(&(dl - p.now).to_le_bytes());
            sigs[k].extend_from_slice(&n.to_le_bytes());
        }
        sigs[k].push(0xff);
    }
    let mut a = sigs;
    a.sort_unstable();
    a.dedup();
    let mut b = levels;
    b.sort_unstable();
    b.dedup();
    (a.len(), b.len())
}

fn suffix_index(name: &str) -> Option<usize> {
    name.rsplit_once('#').and_then(|(_, k)| k.parse().ok())
}

/// The same plant with every transport's trip home deleted, which is exactly
/// what a v2 `link` was: a vehicle that unloads and is instantly available to
/// load again, half a factory away.
fn with_teleporting_links(bp: &Blueprint) -> Blueprint {
    let mut b = bp.clone();
    for a in &mut b.actors {
        a.return_latency = 0;
    }
    b.base_period = b.actors.iter().fold(1u64, |p, a| lcm(p, a.cycle()));
    b
}

/// The same plant with every storage switched to one arbitration rule.
///
/// `Index` reproduces v1: the storage serves its clients in declaration order,
/// so whoever was written first is fed first and the last one in the file eats
/// whatever survives. Nothing in v1 chose that; it was the order of a `Vec`.
fn with_policy(bp: &Blueprint, pol: Policy) -> Blueprint {
    let mut b = bp.clone();
    for s in &mut b.storages {
        s.policy = pol;
        s.order = s.clients.clone();
        s.takers.sort_unstable();
        s.givers.sort_unstable();
    }
    b
}

// ------------------------------------------------------------- prototype 1

/// Play a scenario without a browser.
///
/// `--buy Name=N@T` retunes a machine class to `N` members at tick `T`, which
/// is the only kind of purchase worth making from a command line and quite
/// enough to demonstrate the thing: an edit at tick T, the plant carrying its
/// state across it, and the order met or not met at the deadline.
fn play(path: &str, args: &[String]) -> Result<(), String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let sc = scenario::parse(&src).map_err(|e| format!("{path}: {e}"))?;

    let plant = format!("configs/{}", sc.plant);
    let psrc = std::fs::read_to_string(&plant).map_err(|e| format!("cannot read {plant}: {e}"))?;
    let prog = dsl::parse(&psrc).map_err(|e| format!("{plant}: {e}"))?;
    let mut base = Graph::from_program(&prog);
    base.apply_positions(&psrc);
    let mut log = Log::new(base);

    let mut at = sc.orders.iter().map(|o| o.deadline()).max().unwrap_or(60_000);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--at" => {
                at = args
                    .get(i + 1)
                    .and_then(|v| v.replace('_', "").parse().ok())
                    .ok_or("--at needs a tick")?;
                i += 2;
            }
            "--buy" => {
                let spec = args.get(i + 1).ok_or("--buy needs Name=N@Tick")?;
                let (name, rest) = spec.split_once('=').ok_or("--buy needs Name=N@Tick")?;
                let (count, tick) = rest.split_once('@').ok_or("--buy needs Name=N@Tick")?;
                let count: u64 = count.parse().map_err(|_| format!("`{count}` is not a count"))?;
                let tick: Tick = tick.parse().map_err(|_| format!("`{tick}` is not a tick"))?;
                let mut node = log
                    .base
                    .node(name)
                    .ok_or(format!("`{name}` is not in {}", sc.plant))?
                    .clone();
                node.count = count;
                log.commands.push(Command { at: tick, edit: Edit::Retune(node) });
                i += 2;
            }
            other => return Err(format!("`{other}` is not a play option")),
        }
    }
    log.commands.sort_by_key(|c| c.at);

    rule('=');
    println!("{}  --  {}", sc.name, sc.plant);
    rule('=');
    println!("{}", sc.brief);
    println!();

    let started = Instant::now();
    let verdict = scenario::evaluate(&sc, &log, at).map_err(|e| match e.at {
        Some(t) => format!("the command at t={}: {}", commas(t as u128), e.msg),
        None => e.msg,
    })?;
    let elapsed = started.elapsed();

    println!("at tick {}", commas(at as u128));
    println!();
    for c in &log.commands {
        println!("  t={:>12}  {} {}", commas(c.at as u128), c.edit.verb(), c.edit.subject());
    }
    if !log.commands.is_empty() {
        println!();
    }

    // What the plant is doing, and why it is not doing more.
    live::with_state(&log, at, |a| {
        let snap = temporal_rooms::snap::render(a.prog, a.bp, a.plan, a.room, at);
        println!("  {:<12} {:<18} {}", "class", "state", "why");
        for c in snap.at("classes").as_arr() {
            let w = c.at("why");
            println!(
                "  {:<12} {:<18} {}",
                c.at("name").as_str().unwrap_or(""),
                w.at("state").as_str().unwrap_or(""),
                w.at("headline").as_str().unwrap_or(""),
            );
        }
        println!();
        let cons = snap.at("constraints").as_arr();
        if cons.is_empty() {
            println!("  nothing is flat out while something waits on it");
        } else {
            for c in cons {
                println!(
                    "  holding the plant back: {} at {:.3}/tick, starving {}",
                    c.at("name").as_str().unwrap_or(""),
                    c.at("rate").as_f64().unwrap_or(0.0),
                    c.at("starving")
                        .as_arr()
                        .iter()
                        .filter_map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        for s in a.scrapped {
            println!("  scrapped: {} -- {}", s.what, s.detail);
        }
    })
    .map_err(|e| e.msg)?;

    println!();
    println!(
        "  budget {}   spent {}   left {}",
        commas(sc.budget as u128),
        commas(verdict.at("spent").as_u64().unwrap_or(0) as u128),
        commas(verdict.at("remaining").as_u64().unwrap_or(0) as u128),
    );
    if let Some(t) = verdict.at("overspent").as_u64() {
        println!("  OVER BUDGET from t={}", commas(t as u128));
    }
    println!();
    for o in verdict.at("orders").as_arr() {
        let have = o.at("have").as_u64().unwrap_or(0);
        let need = o.at("need").as_u64().unwrap_or(1);
        println!(
            "  [{}] {}",
            if o.at("met").as_bool() == Some(true) {
                "MET "
            } else if o.at("failed").as_bool() == Some(true) {
                "MISS"
            } else {
                " .. "
            },
            o.at("text").as_str().unwrap_or(""),
        );
        println!(
            "         {} of {}  ({:.1}%)",
            commas(have as u128),
            commas(need as u128),
            100.0 * have as f64 / need.max(1) as f64
        );
    }
    println!();

    // The same tick, reached from a snapshot halfway through: the networking
    // proof rehearsed against itself, for free, every time anyone plays.
    let whole = live::carry_at(&log, at).map_err(|e| e.msg)?;
    let mid = live::carry_at(&log, at / 2).map_err(|e| e.msg)?;
    let joined = live::with_state_from(&log, at, Some((at / 2, &mid)), |a| {
        live::Carry::take(a.room, a.prog, a.bp, at)
    })
    .map_err(|e| e.msg)?;
    println!(
        "  replay from a snapshot at t={}: {}",
        commas((at / 2) as u128),
        if whole.signature() == joined.signature() { "identical" } else { "DESYNC" }
    );
    println!("  answered in {:?}", elapsed);
    rule('=');
    Ok(())
}

// ------------------------------------------------------------ formatting

fn rule(ch: char) {
    println!("{}", std::iter::repeat(ch).take(64).collect::<String>());
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

fn bytes_h(n: u128) -> String {
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

#[allow(dead_code)]
fn unused(_: sim::Ev) {}
