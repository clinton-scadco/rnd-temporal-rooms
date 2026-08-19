//! Experiment harness for v2.
//!
//! v1 asked whether a billion *independent* factory objects could be answered
//! without touching a billion objects. They could. v2 removes the independence
//! -- shared buffers, fan-in, fan-out, feedback cycles, batch transport -- and
//! asks the same question again.
//!
//! Every configuration is put through the same gauntlet, and every analytic
//! answer is checked against the event simulator that is not allowed to cheat.

use std::time::{Duration, Instant};
use temporal_rooms::analytic::{self, Rat};
use temporal_rooms::domains;
use temporal_rooms::dsl;
use temporal_rooms::model::*;
use temporal_rooms::pop;
use temporal_rooms::sim::{self, CountersBig, World};

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
    ]
}

fn adhoc(path: &str) -> Cfg {
    cfg(path, path, 3_000, 2_000, &[1_000, 3_000])
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfgs: Vec<Cfg> =
        if args.is_empty() { configs() } else { args.iter().map(|a| adhoc(a)).collect() };

    let mut failures = 0usize;
    let mut summary: Vec<String> = Vec::new();
    for c in &cfgs {
        match run(c) {
            Ok(line) => summary.push(line),
            Err(n) => {
                failures += n;
                summary.push(format!("{:<16} {:>12} FAILED", short(&c.path), ""));
            }
        }
    }

    rule('=');
    println!("SUMMARY");
    rule('=');
    println!(
        "{:<16} {:>15} {:>9} {:>11} {:>10} {:>11}",
        "config", "objects", "classes", "pop cells", "T5 solve", "compression"
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
            let duty = cyc.mul(Rat::new(ad.duration as u128, 1)).div(Rat::new(ad.count as u128, 1));
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
                                .mul(Rat::new(b2.actors[a].duration as u128, 1))
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
        let dm = Deploy { blueprint: d.blueprint, count: n_mat, stagger: d.stagger };
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
        "{:<16} {:>15} {:>9} {:>11} {:>10} {:>11}",
        short(&c.path),
        commas(prog.total_objects()),
        bp.actors.len(),
        pf.max_distinct_states,
        dur(t5_time),
        format!("{:.0}x", pf.population as f64 / pf.max_distinct_states.max(1) as f64)
    );
    if fails > 0 {
        Err(fails)
    } else {
        Ok(line)
    }
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
