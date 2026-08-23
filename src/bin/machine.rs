//! Experiments 06 and 07, from a terminal.
//!
//! The designer is a browser tool, but a design is a text file and a verdict is
//! a table, so neither of those should need a browser. This binary is how the
//! experiment gets into a test, a shell pipe or a commit message.
//!
//! ```text
//!   machine                        every design in ./designs, judged
//!   machine run FILE [--at T]      one design: the verdict, and its state at T
//!   machine why FILE [--at T]      what every component is doing, and why
//!   machine compile FILE           the macro-machine, and its orbit
//!   machine verify FILE [T...]     the compiled answer against a straight run
//!   machine parts [FAMILY]         the vocabulary: ports, recipes, constraints
//!   machine reuse                  which primitives earned their place
//!   machine serve [--port N]       the designer
//! ```
//!
//! `reuse` is experiment 07's own acceptance test and the reason it is a
//! command rather than a paragraph. The note that asked for the experiment was
//! explicit: if the same motor, pump, exchanger, buffer and shaft appear across
//! several designs the primitives are good, and if every challenge needs ten
//! bespoke components used nowhere else the abstraction is wrong. That is a
//! countable claim, so it is counted.

use temporal_rooms::machine::design::Design;
use temporal_rooms::machine::parts::{self, Family};
use temporal_rooms::machine::sim::Tick;
use temporal_rooms::machine::{eval, orbit, snap, web};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("all");
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };

    let code = match cmd {
        "serve" => {
            let port = flag(rest, "--port").and_then(|s| s.parse().ok()).unwrap_or(8788);
            match web::serve(port) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("cannot serve on port {port}: {e}");
                    1
                }
            }
        }
        "run" => run(rest),
        "why" => why(rest),
        "compile" => compile(rest),
        "verify" => verify(rest),
        "parts" => catalogue(rest),
        "reuse" => reuse(),
        "all" => all(),
        other if other.ends_with(".machine") => run(&args),
        other => {
            eprintln!(
                "`{other}` is not a command. Try `run`, `why`, `compile`, `verify`, \
                 `parts`, `reuse` or `serve`."
            );
            2
        }
    };
    std::process::exit(code);
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

fn load(args: &[String]) -> Result<(String, Design), String> {
    let path = args
        .iter()
        .find(|a| !a.starts_with("--") && a.parse::<u64>().is_err())
        .ok_or("which design?")?;
    let src = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let d = Design::parse(&src).map_err(|e| format!("{path}: {e}"))?;
    let faults = d.check();
    if let Some(f) = faults.first() {
        return Err(format!("{path}: {}", f.what));
    }
    Ok((path.clone(), d))
}

fn bail(e: String) -> i32 {
    eprintln!("{e}");
    1
}

// --------------------------------------------------------------- the verdict

fn run(args: &[String]) -> i32 {
    let (path, d) = match load(args) {
        Ok(v) => v,
        Err(e) => return bail(e),
    };
    let c = match orbit::compile(&d) {
        Ok(c) => c,
        Err(e) => return bail(e),
    };
    let r = eval::report(&d, &c);
    println!("{path}\n");
    print!("{}", r.text());

    if let Some(t) = flag(args, "--at").and_then(|s| s.parse::<Tick>().ok()) {
        let totals = c.totals_at(t);
        println!(
            "\n  at t={t} (simulated as t={})\n    {:<18}{:>16}\n    {:<18}{:>16}\n    {:<18}{:>16}\n    {:<18}{:>16}",
            c.equivalent_tick(t),
            "MW-ticks",
            commas(totals.power),
            "fuel",
            commas(totals.fuel()),
            "water",
            commas(totals.water()),
            "heat wasted",
            commas(totals.heat_wasted),
        );
    }
    if r.met() {
        0
    } else {
        1
    }
}

/// Every component, and why it is doing what it is doing. This is the panel the
/// browser draws, printed.
fn why(args: &[String]) -> i32 {
    let (path, d) = match load(args) {
        Ok(v) => v,
        Err(e) => return bail(e),
    };
    let t: Tick = flag(args, "--at").and_then(|s| s.parse().ok()).unwrap_or(4_000);
    let c = match orbit::compile(&d) {
        Ok(c) => c,
        Err(e) => return bail(e),
    };
    let m = match c.state_at(&d, t) {
        Ok(m) => m,
        Err(e) => return bail(e),
    };
    println!("{path} at t={t}\n");
    for i in 0..m.len() {
        println!(
            "{:<10} {:<20} {}",
            m.names[i],
            temporal_rooms::machine::parts::part(m.kinds[i]).title,
            m.st[i].status.tag()
        );
        for line in snap::why(&d, &m, i) {
            println!("    {line}");
        }
        println!();
    }
    0
}

fn compile(args: &[String]) -> i32 {
    let (path, d) = match load(args) {
        Ok(v) => v,
        Err(e) => return bail(e),
    };
    let c = match orbit::compile(&d) {
        Ok(c) => c,
        Err(e) => return bail(e),
    };
    let r = eval::report(&d, &c);
    let m = eval::macro_machine(&d, &c, &r);
    println!("{path}\n");
    println!("{}\n", m.to_string());

    if c.settled() {
        // The claim, and what it is worth: a tick a billion in the future costs
        // at most this many steps to answer exactly.
        let far: Tick = 1_000_000_000;
        println!(
            "  tick {} is indistinguishable from tick {} — {} steps, not {}",
            commas(far as u128),
            commas(c.equivalent_tick(far) as u128),
            commas(c.equivalent_tick(far) as u128),
            commas(far as u128)
        );
        let t = c.totals_at(far);
        println!(
            "  by then: {} MW-ticks, {} fuel, {} water, {} heat wasted",
            commas(t.power),
            commas(t.fuel()),
            commas(t.water()),
            commas(t.heat_wasted)
        );
    }
    0
}

fn verify(args: &[String]) -> i32 {
    let (path, d) = match load(args) {
        Ok(v) => v,
        Err(e) => return bail(e),
    };
    let mut ticks: Vec<Tick> = args.iter().filter_map(|a| a.parse().ok()).collect();
    if ticks.is_empty() {
        ticks = vec![1, 60, 119, 121, 500, 3_000, 12_345, 100_000];
    }
    let checks = match orbit::verify(&d, &ticks) {
        Ok(c) => c,
        Err(e) => return bail(e),
    };
    println!("{path}\n");
    println!("  {:>10}  {:>18}  {:>18}  {}", "tick", "simulated", "compiled", "");
    let mut bad = 0;
    for c in &checks {
        if !c.agrees {
            bad += 1;
        }
        println!(
            "  {:>10}  {:>18}  {:>18}  {}",
            commas(c.tick as u128),
            commas(c.simulated.power),
            commas(c.compiled.power),
            if c.agrees { "ok" } else { "DIFFERS" }
        );
    }
    println!(
        "\n  {} probes, {}",
        checks.len(),
        if bad == 0 { "all agree".to_string() } else { format!("{bad} DISAGREE") }
    );
    if bad == 0 {
        0
    } else {
        1
    }
}

/// Every design on disk, in one table, grouped by the brief it answers.
///
/// This is the only view in which the two claims of experiment 07 are visible
/// at once: that there are genuinely different solutions to the same brief, and
/// that there are four different briefs answered by one set of components.
///
/// A design that misses its brief is not an error here. Half of what is in
/// `designs/` is there precisely because it misses, and the exit code is about
/// whether the files could be read and run.
fn all() -> i32 {
    let paths = design_paths();
    if paths.is_empty() {
        eprintln!("no designs in ./designs");
        return 1;
    }
    let mut loaded: Vec<(String, Design)> = Vec::new();
    let mut worst = 0;
    for path in &paths {
        let parsed = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|src| Design::parse(&src));
        match parsed {
            Ok(d) => loaded.push((path.clone(), d)),
            Err(e) => {
                println!("{:<26} {e}", short(path));
                worst = 1;
            }
        }
    }

    for brief in eval::BRIEFS {
        let mine: Vec<&(String, Design)> =
            loaded.iter().filter(|(_, d)| d.brief == brief).collect();
        if mine.is_empty() {
            continue;
        }
        println!("\n{}  --  {}", brief.title().to_uppercase(), brief.tests());
        println!("{}", brief.goal());
        println!(
            "\n{:<26} {:>9} {:>8} {:>7} {:>7} {:>8} {:>6} {:>5} {:>6} {:>8}",
            "design", "made", "grid", "water", "wasted", "plot", "parts", "util", "start",
            "period"
        );
        println!("{}", "-".repeat(104));
        for (path, d) in mine {
            let c = match orbit::compile(d) {
                Ok(c) => c,
                Err(e) => {
                    println!("{:<26} {e}", short(path));
                    worst = 1;
                    continue;
                }
            };
            let r = eval::report(d, &c);
            println!(
                "{:<26} {:>9.2} {:>8.1} {:>7.1} {:>7.1} {:>8} {:>6} {:>4.0}% {:>6} {:>8}",
                short(path),
                r.headline().value(),
                r.grid.value(),
                r.water.value(),
                r.wasted.value(),
                format!("{}x{}", r.width, r.height),
                r.components,
                r.util.value(),
                r.transient,
                if r.settled { r.period.to_string() } else { "--".into() }
            );
            for f in &r.failings {
                println!("{:<26} ! {f}", "");
            }
        }
    }
    println!(
        "\nThere is no single score, in any of the four. A compact answer and a \
         clean answer are different machines, and that is the whole point."
    );
    worst
}

/// The vocabulary itself: what each component takes, refuses and makes.
///
/// With thirty-eight of them the table in `parts.rs` is no longer something you
/// can hold in your head, and a player who has to read Rust to find out that a
/// rolling mill will not touch cold metal has been failed by the tool.
fn catalogue(args: &[String]) -> i32 {
    let only = args.first().map(|s| s.as_str());
    let mut shown = 0;
    for family in [
        Family::Source,
        Family::Sink,
        Family::Transport,
        Family::Store,
        Family::Control,
        Family::Heat,
        Family::Mechanical,
        Family::Process,
    ] {
        if let Some(f) = only {
            if f != family.tag() {
                continue;
            }
        }
        println!("\n{}", family.tag().to_uppercase());
        for &kind in parts::KINDS.iter().filter(|k| k.family() == family) {
            let p = parts::part(kind);
            shown += 1;
            println!("\n  {:<11} {:<22} {}x{}", p.tag, p.title, p.w, p.h);
            println!("  {:<11} {}", "", p.blurb);
            let ports: Vec<String> = p
                .ports
                .iter()
                .map(|q| {
                    format!(
                        "{}{} {} {}/tick",
                        if q.external { "*" } else { "" },
                        q.name,
                        q.dom,
                        q.rate
                    )
                })
                .collect();
            println!("  {:<11} {}", "ports", ports.join("   "));
            if let Some(r) = p.recipe {
                for dr in r.draws {
                    let mut line =
                        format!("{} {}/tick", dr.qty * r.rate, p.ports[dr.port].name);
                    if !dr.need.is_empty() {
                        let all: Vec<String> = dr.need.iter().map(|n| n.wants()).collect();
                        line.push_str(&format!("  ({})", all.join(", ")));
                    }
                    println!("  {:<11} {line}", "takes");
                }
                for mk in r.makes {
                    let mut line =
                        format!("{} {}/tick", mk.qty * r.rate, p.ports[mk.port].name);
                    if !mk.eff.is_empty() {
                        let all: Vec<String> = mk.eff.iter().map(|e| e.said()).collect();
                        line.push_str(&format!("  ({})", all.join(", ")));
                    }
                    println!("  {:<11} {line}", "makes");
                }
            } else {
                println!("  {:<11} {}", "behaviour", "hand written -- see `sim`");
            }
        }
    }
    if shown == 0 {
        eprintln!(
            "no such family. Try source, sink, transport, store, control, heat, \
             mechanical or process."
        );
        return 2;
    }
    println!(
        "\n  * marks the machine's boundary: what is left in one of those ports \
         leaves the machine, as product or as waste."
    );
    0
}

/// Which primitives earned their place.
///
/// A component that appears in one design is a bespoke component wearing a
/// costume; one that appears across several briefs is a primitive. The note
/// that asked for experiment 07 proposed exactly this test, so here it is,
/// counted rather than asserted.
fn reuse() -> i32 {
    let mut designs: Vec<Design> = Vec::new();
    for path in design_paths() {
        if let Ok(src) = std::fs::read_to_string(&path) {
            if let Ok(d) = Design::parse(&src) {
                designs.push(d);
            }
        }
    }
    if designs.is_empty() {
        eprintln!("no designs in ./designs");
        return 1;
    }
    let uses = eval::reuse(&designs);
    println!(
        "{:<12} {:<11} {:>8} {:>8}  {}",
        "component", "family", "designs", "placed", "briefs"
    );
    println!("{}", "-".repeat(78));
    let mut unused = Vec::new();
    for u in &uses {
        if u.designs == 0 {
            unused.push(u.kind.tag());
            continue;
        }
        println!(
            "{:<12} {:<11} {:>8} {:>8}  {}",
            u.kind.tag(),
            u.kind.family().tag(),
            u.designs,
            u.placed,
            u.briefs.iter().map(|b| b.tag()).collect::<Vec<_>>().join(" ")
        );
    }

    // The two halves are judged differently, and saying so is the point. A
    // crusher belongs to the crush brief the way a verb belongs to a sentence:
    // nobody expected it to turn up in a refinery. Infrastructure is the half
    // the claim was ever about -- if the motor, pump, exchanger, buffer and
    // shaft that a power plant needed are also what an ore line needs, the
    // vocabulary is a vocabulary.
    let infra: Vec<&eval::Uses> =
        uses.iter().filter(|u| u.kind.family() != Family::Process).collect();
    let process: Vec<&eval::Uses> =
        uses.iter().filter(|u| u.kind.family() == Family::Process).collect();
    let spanning = |v: &[&eval::Uses]| v.iter().filter(|u| u.briefs.len() > 1).count();
    let used = |v: &[&eval::Uses]| v.iter().filter(|u| u.designs > 0).count();

    println!(
        "
  infrastructure   {} of {} used, {} of them across more than one brief",
        used(&infra),
        infra.len(),
        spanning(&infra)
    );
    println!(
        "  process          {} of {} used, {} of them across more than one brief",
        used(&process),
        process.len(),
        spanning(&process)
    );
    println!();
    println!("  A process component belonging to one brief is not a failure -- a");
    println!("  crusher belongs to the crush brief the way a verb belongs to a");
    println!("  sentence. What the claim was ever about is the infrastructure, and a");
    println!("  reactor built for a power plant now heats a distillation column.");
    if !unused.is_empty() {
        println!();
        println!("  used by no shipped design: {}", unused.join(", "));
        println!("  Mostly stores and controls, and the reason is one sentence: every");
        println!("  port already has a capacity, so every component is already a buffer");
        println!("  and a dedicated one has nothing left to do. The fix is smaller");
        println!("  capacities, not more components.");
    }
    0
}

fn design_paths() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Ok(dir) = std::fs::read_dir("designs") {
        for e in dir.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "machine") {
                names.push(p.to_string_lossy().into_owned());
            }
        }
    }
    names.sort();
    names
}

fn short(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).replace(".machine", "")
}

fn commas(n: u128) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
