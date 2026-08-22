//! Experiment 06, from a terminal.
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
//!   machine serve [--port N]       the designer
//! ```

use temporal_rooms::machine::design::Design;
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
        "all" => all(),
        other if other.ends_with(".machine") => run(&args),
        other => {
            eprintln!("`{other}` is not a command. Try `run`, `why`, `compile`, `verify` or `serve`.");
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
            commas(totals.fuel),
            "water",
            commas(totals.water),
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
            commas(t.fuel),
            commas(t.water),
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

/// Every design on disk, in one table, which is the only view in which "these
/// are genuinely different solutions to the same brief" is visible at a glance.
fn all() -> i32 {
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
    if names.is_empty() {
        eprintln!("no designs in ./designs");
        return 1;
    }
    println!(
        "{:<26} {:>8} {:>7} {:>7} {:>8} {:>7} {:>5} {:>6} {:>9}",
        "design", "MW", "water", "wasted", "plot", "parts", "util", "start", "period"
    );
    println!("{}", "-".repeat(94));
    let mut worst = 0;
    for path in &names {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                println!("{path:<26} cannot read: {e}");
                worst = 1;
                continue;
            }
        };
        let d = match Design::parse(&src) {
            Ok(d) => d,
            Err(e) => {
                println!("{:<26} {e}", short(path));
                worst = 1;
                continue;
            }
        };
        let c = match orbit::compile(&d) {
            Ok(c) => c,
            Err(e) => {
                println!("{:<26} {e}", short(path));
                worst = 1;
                continue;
            }
        };
        let r = eval::report(&d, &c);
        println!(
            "{:<26} {:>8.2} {:>7.1} {:>7.1} {:>8} {:>7} {:>4.0}% {:>6} {:>9}",
            short(path),
            r.power.value(),
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
    println!(
        "\nthe brief: at least {} MW from one fuel source, on the smallest plot,\n\
         with the least water and the least wasted heat. There is no single score.",
        eval::TARGET_MW
    );
    worst
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
