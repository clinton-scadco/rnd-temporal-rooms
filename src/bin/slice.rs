//! Experiment 15, from a terminal.
//!
//! ```text
//!   slice serve [--port N] [--host A]   three centuries, in a browser
//!   slice play  [--seed N] [--wires]    the whole slice, played headlessly
//!   slice map   [--phase 1890] [--png F]  the plot, one character per tile
//!   slice land                          one rectangle, three centuries, in a table
//!   slice phases                        what each century can build, and what it cannot
//!   slice cross                         the two fractures, and what holds them open
//!   slice price [--part motor] [--into 1890]   what a crate of machinery costs
//!   slice refuse                        everything the slice will not allow
//! ```
//!
//! `play` is this experiment's acceptance command, in the same sense that `camp
//! play` is Prototype 3's. It mines 1890, bootstraps a 2037 grid off the last of
//! a coal seam and a river, opens a hundred-and-forty-seven-year fracture,
//! carries the ore forward, crushes it with machinery that will not exist for a
//! century, presses gears out of the result, carries *those* backwards, and
//! stands 1890's own power station up out of two of them. Then it asks the three
//! questions the experiment exists to answer:
//!
//! ```text
//!   did every region meet an objective it could not have met alone?
//!   is the 2037 concentrate made of 1890 ore, and can the slice say so?
//!   did every replica of every region agree with its host, throughout?
//! ```
//!
//! `serve` is the other half: three centuries in a browser, on
//! [`slice::net`](temporal_rooms::slice::net), with Prototype 2's region view
//! served unforked underneath and the ground of whichever century you are
//! standing in painted behind it.
//!
//! `map` remains, and it is not a lesser version of that. The claim "crossing a
//! fracture makes the world change clearly" is a claim somebody can be wrong
//! about, so it is rendered -- as glyphs for a terminal and as pixels for a PNG
//! -- and then *counted*, because a picture two people like is not evidence and
//! a screenshot cannot be put in a test.

use temporal_rooms::machine::design::{Design, Tune};
use temporal_rooms::machine::era::MATS;
use temporal_rooms::machine::form::shot;
use temporal_rooms::machine::parts;
use temporal_rooms::mp::goal::{commas, Goal};
use temporal_rooms::mp::lower::item_title;
use temporal_rooms::slice::land;
use temporal_rooms::slice::phase::{self, Phase, PHASES};
use temporal_rooms::slice::play::{self, Play};
use temporal_rooms::slice::region::REGIONS;
use temporal_rooms::slice::{gate, run::Slice};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("play");
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    let code = match cmd {
        "serve" => {
            let port = flag(rest, "--port").and_then(|s| s.parse().ok()).unwrap_or(8797);
            // Loopback by default: a world left running is not a thing to put
            // on the network without saying so, and `--host 0.0.0.0` is how you
            // say so.
            let host = flag(rest, "--host").unwrap_or("127.0.0.1");
            match temporal_rooms::slice::net::serve(host, port) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("cannot serve on {host} port {port}: {e}");
                    1
                }
            }
        }
        "play" => play(flag(rest, "--seed").and_then(|s| s.parse().ok()).unwrap_or(3)),
        "map" => map(flag(rest, "--phase"), flag(rest, "--png")),
        "land" => land_table(),
        "phases" => phases(),
        "cross" => crossings(),
        "price" => price(flag(rest, "--part"), flag(rest, "--into")),
        "refuse" => refusals(),
        other => {
            eprintln!(
                "`{other}` is not a command. Try `serve`, `play`, `map`, `land`, `phases`, \
                 `cross`, `price` or `refuse`."
            );
            2
        }
    };
    std::process::exit(code);
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn rule(title: &str) {
    println!("\n\x1b[1m{title}\x1b[0m");
    println!("{}", "-".repeat(title.len().max(8)));
}

// ==================================================================== the run

fn play(seed: u64) -> i32 {
    let mut p = Play::open(seed);
    play::run(&mut p);
    finish(p)
}

/// What happened, and whether any of it was wrong.
fn finish(p: Play) -> i32 {
    rule("the slice");
    for r in REGIONS {
        let y = p.s.yard(r.tag).expect("a region of its own slice");
        let met = match y.done_at() {
            Some(t) => format!("met at {}", play::clock(t)),
            None => "not met".to_string(),
        };
        let (landed, standing, free) = p.s.machinery(r.tag);
        println!(
            "  {:<10}{:<28} {:<14} {:>3} MW on the grid",
            r.phase.tag(),
            r.title,
            met,
            y.grid_mw()
        );
        println!(
            "            {:>3} machines, {:>5} tiles, machinery: {} landed, {} standing, {} free",
            y.room.host.world.installs.len(),
            y.room.host.world.footprint(),
            commas(landed),
            commas(standing),
            free
        );
        // What it has actually delivered, by item. The number every objective is
        // scored on, every outbound load is drawn from, and every fracture is
        // held open with -- so it is the first thing worth printing.
        let shipped: Vec<String> = y
            .room
            .host
            .acct
            .shipped
            .iter()
            .filter(|(_, n)| **n > 0)
            .map(|(item, n)| format!("{} {}", commas(*n), item_title(item)))
            .collect();
        if !shipped.is_empty() {
            println!("            shipped: {}", shipped.join(", "));
        }
        // `--wires` prints the plant as a graph. It is here because it is what
        // found the two hardest bugs in this experiment: a stamping line whose
        // billet was being swallowed by the depot standing next to it, and a
        // compact plant that had grown into its own tailrace on commit. In a
        // world with three regions in it, "which of these is wired to what" is
        // not a question anybody should have to answer by reading a script.
        if std::env::args().any(|a| a == "--wires") {
            for c in &y.room.host.world.conns {
                let n =
                    |id| y.room.host.world.get(id).map(|i| i.name.clone()).unwrap_or("?".into());
                println!("              {} -> {} : {}", n(c.from), n(c.to), c.item);
            }
        }
        // Whatever is not running, and the region's own words for why. Printed
        // whether or not the run went well: a slice that met every objective and
        // left a stamping line starving is still a slice worth looking at.
        for (id, why) in &y.room.host.build.idle {
            let name = y.room.host.world.get(*id).map(|i| i.name.clone()).unwrap_or_default();
            println!("            \x1b[33m{name} is idle: {why}\x1b[0m");
        }
    }

    rule("the fractures");
    for g in &p.s.ledger.gates {
        let f = g.fracture();
        println!(
            "  {:<8}{:>4} years  {:>4} MW wanted  {:>4} MW had  {:>10} carried  {}",
            f.tag,
            g.worst,
            g.want_mw,
            g.had_mw,
            commas(g.carried),
            if g.lit { "\x1b[32mlit\x1b[0m" } else { "\x1b[31mdark\x1b[0m" }
        );
    }

    rule("what crossed, and what it was made of");
    for ((tag, item), bin) in &p.s.ledger.bins {
        if bin.landed.is_empty() {
            continue;
        }
        let from: Vec<String> =
            bin.landed.iter().map(|(p, n)| format!("{} out of {}", commas(*n), p.tag())).collect();
        println!("  {:<10}{:<14}{}", tag, item, from.join(", "));
    }

    rule("the run");
    println!("  {} seconds of simulated time", p.t);
    println!("  {} synchronisation checks", p.checks);
    if p.bad.is_empty() {
        println!("\n  \x1b[32mnothing went wrong\x1b[0m");
        0
    } else {
        println!("\n  \x1b[31m{} things went wrong\x1b[0m", p.bad.len());
        for b in &p.bad {
            println!("    {b}");
        }
        1
    }
}

// =================================================================== the maps

fn map(which: Option<&str>, png: Option<&str>) -> i32 {
    let want: Vec<Phase> = match which {
        None => PHASES.to_vec(),
        Some(tag) => match Phase::by_tag(tag) {
            Some(p) => vec![p],
            None => {
                eprintln!("`{tag}` is not a phase. Try 1890, 2037 or 2070.");
                return 2;
            }
        },
    };
    for p in &want {
        let r = REGIONS.iter().find(|r| r.phase == *p);
        rule(&format!(
            "{} -- {}",
            r.map(|r| r.title).unwrap_or(p.tag()),
            p.blurb()
        ));
        print!("{}", land::glyphs(*p));
        let legend: Vec<String> =
            land::legend(*p).iter().map(|(g, what)| format!("{g} {what}")).collect();
        println!("\n  {}", legend.join("   "));
    }
    if want.len() > 1 {
        rule("what a crossing changes");
        for (a, b) in [(Phase::P1890, Phase::P2037), (Phase::P2037, Phase::P2070)] {
            let c = land::changes(a, b);
            let by: Vec<String> =
                c.by_layer.iter().map(|(l, n)| format!("{} {}", n, l.tag())).collect();
            println!(
                "  {} -> {}   {:>5} of {} tiles ({}%)   {}",
                a.tag(),
                b.tag(),
                c.tiles,
                c.total,
                c.pct(),
                by.join(", ")
            );
        }
    }
    if let Some(path) = png {
        // Three panels, side by side, at four pixels a tile: the same fold as
        // the glyphs, so a picture and a terminal cannot disagree.
        let scale = 4;
        let (w, h, _) = land::rgb(Phase::P1890, scale);
        let gap = 8;
        let (tw, th) = (w * 3 + gap * 2, h);
        let mut buf = vec![24u8; tw * th * 3];
        for (k, p) in PHASES.iter().enumerate() {
            let (_, _, panel) = land::rgb(*p, scale);
            let x0 = k * (w + gap);
            for y in 0..h {
                let src = y * w * 3;
                let dst = (y * tw + x0) * 3;
                buf[dst..dst + w * 3].copy_from_slice(&panel[src..src + w * 3]);
            }
            // The year, on the panel. A picture of three centuries of one place
            // gets looked at a long way from the terminal that made it, and a
            // contact sheet whose panels are not labelled is three squares.
            year(&mut buf, tw, x0 + 4, 4, p.tag(), 3);
        }
        match std::fs::write(path, shot::png(tw, th, &buf)) {
            Ok(()) => println!(
                "\n  wrote {path}: {tw} x {th} -- 1890, 2037 and 2070, left to right"
            ),
            Err(e) => {
                eprintln!("cannot write {path}: {e}");
                return 1;
            }
        }
    }
    0
}

/// One rectangle, three centuries, read across rather than down.
fn land_table() -> i32 {
    rule(&format!("{0} x {0} tiles, written once", land::PLOT));
    println!(
        "  {:<18}{:<9}{:<26}{:<26}{}",
        "feature", "at", "1890", "2037", "2070"
    );
    for f in land::LAND {
        let say = |p: Phase| -> String {
            let face = f.face(p);
            match face.yields() {
                Some((item, q)) => format!("{} {}/s", item_title(item), q),
                None => face.title().to_string(),
            }
        };
        println!(
            "  {:<18}{:<9}{:<26}{:<26}{}",
            f.name,
            format!("{},{}", f.x, f.y),
            say(Phase::P1890),
            say(Phase::P2037),
            say(Phase::P2070)
        );
    }
    rule("the one the brief asks for");
    for tag in ["kestrel", "kestrel-spoil"] {
        if let Some(f) = land::feature(tag) {
            println!("\n  \x1b[1m{}\x1b[0m at {},{}  {} x {}", f.name, f.x, f.y, f.w, f.h);
            for p in PHASES {
                let face = f.face(p);
                println!(
                    "    {}  {:<34}{}",
                    p.tag(),
                    face.title(),
                    match face.yields() {
                        Some((i, q)) => format!("{} {}/s", item_title(i), q),
                        None if face.taken() => "nothing may be built here".to_string(),
                        None => String::new(),
                    }
                );
            }
            println!("    {}", wrap(f.became, 74, "    "));
        }
    }
    0
}

// ================================================================ the phases

fn phases() -> i32 {
    for p in PHASES {
        let r = REGIONS.iter().find(|r| r.phase == p);
        rule(&format!("{} -- {}", p.tag(), p.role()));
        println!("  {}", wrap(p.blurb(), 74, "  "));
        println!();
        println!("  era gravity      {}", p.era().title());
        println!("  a grid to use    {}", if p.grid() { "yes" } else { "no" });
        println!(
            "  frames it makes  {}",
            MATS.iter()
                .filter(|m| p.mats().contains(m))
                .map(|m| m.title())
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!("  components       {} of {}", p.holds().len(), parts::KINDS.len());
        let lacks = p.lacks();
        if lacks.is_empty() {
            println!("  it lacks         nothing at all");
        } else {
            println!("  it lacks:");
            for t in &lacks {
                let a = phase::arrival(t);
                let kind = parts::by_tag(t);
                let cost = kind
                    .map(|k| phase::import_cost(k, Tune::default_for(k).mat, p))
                    .unwrap_or(0);
                println!(
                    "    {:<12}{:<14}{}",
                    t,
                    match a {
                        Some(a) if !a.crated => "no crate".to_string(),
                        _ => format!("{} gears", commas(cost)),
                    },
                    wrap(a.map(|a| a.why).unwrap_or(""), 48, &" ".repeat(30))
                );
            }
        }
        if let Some(r) = r {
            println!();
            println!("  {} -- {}", r.title, r.problem);
            let g = Goal::of_seed(1, Some(r.template));
            println!("  objective        {}", g.brief());
            for (item, f, q) in r.ground() {
                println!("    ground         {:<14}{:>6}/s  at {}", item_title(item), q, f.name);
            }
        }
    }
    0
}

// ============================================================= the fractures

fn crossings() -> i32 {
    for f in gate::FRACTURES {
        let (a, b) = f.phases();
        rule(&format!(
            "the {} fracture -- {} to {}, {} years",
            f.tag,
            a.tag(),
            b.tag(),
            f.gap()
        ));
        println!("  {}", wrap(f.why, 74, "  "));
        println!();
        println!("  natural          {}", yes(f.natural));
        println!("  already standing {}", yes(f.standing));
        println!("  held open by     {} ({} MW at its own gap)", f.held_by, gate::draw(f.gap()));
        println!("  gauge            {} a second", commas(gate::gauge(f.gap())));
        println!("  running time     {} seconds at speed 100", f.leagues);
        let c = land::changes(a, b);
        println!(
            "  the world changes {} of {} tiles ({}%)",
            c.tiles,
            c.total,
            c.pct()
        );
        for (l, n) in &c.by_layer {
            println!("    {:<16}{}", l.tag(), n);
        }
    }

    rule("what a crossing costs, by how far out of its time the load is");
    println!("  {:<12}{:<10}{:<12}{}", "years", "MW", "gauge/s", "the case it is");
    for (years, what) in [
        (0, "same phase -- ordinary logistics, no interface at all"),
        (33, "2037 -> 2070, or 2037 goods going the other way"),
        (147, "1890 -> 2037: the deep fracture's own gap"),
        (180, "1890 ore shipped on raw, all the way to 2070"),
    ] {
        if years == 0 {
            println!("  {:<12}{:<10}{:<12}{}", 0, "-", "unlimited", what);
        } else {
            println!(
                "  {:<12}{:<10}{:<12}{}",
                years,
                gate::draw(years),
                commas(gate::gauge(years)),
                what
            );
        }
    }
    println!(
        "\n  Which is the mechanic in one table: crushing 1890 ore in 2037 and sending"
    );
    println!("  concentrate on costs 53 MW, and sending the same ore on raw costs 112.");

    rule("the fleets, and the century that can field one");
    for fl in gate::FLEETS {
        println!(
            "  {:<9}{:<20}{:<7}{:>8} x {}   {}",
            fl.from.tag(),
            fl.title,
            fl.tag,
            commas(fl.load),
            fl.vehicles,
            fl.blurb
        );
    }

    rule("the lanes");
    for l in gate::LANES {
        println!(
            "  {:<10}-> {:<10}{:<14}{}",
            l.from,
            l.to,
            item_title(l.item),
            l.fracture().map(|(_, f)| format!("through the {} fracture", f.tag)).unwrap_or_default()
        );
    }
    0
}

// ================================================================= the price

fn price(part: Option<&str>, into: Option<&str>) -> i32 {
    let ph = match into {
        None => Phase::P1890,
        Some(t) => match Phase::by_tag(t) {
            Some(p) => p,
            None => {
                eprintln!("`{t}` is not a phase. Try 1890, 2037 or 2070.");
                return 2;
            }
        },
    };
    let kinds: Vec<parts::Kind> = match part {
        None => parts::KINDS.to_vec(),
        Some(t) => match parts::by_tag(t) {
            Some(k) => vec![k],
            None => {
                eprintln!("`{t}` is not a component.");
                return 2;
            }
        },
    };
    rule(&format!("what a crate costs in {}", ph.tag()));
    println!(
        "  {} gears per tile of footprint, per fracture it has to come back through.\n",
        phase::CRATE_PER_TILE
    );
    println!(
        "  {:<12}{:<7}{:<7}{:<9}{:<9}{}",
        "component", "tiles", "frame", "made in", "gears", "and if the frame changes"
    );
    let mut free = 0;
    for k in kinds {
        let m = Tune::default_for(k).mat;
        let cost = phase::import_cost(k, m, ph);
        if cost == 0 && ph.builds_on(k, m) {
            free += 1;
            if part.is_some() {
                println!(
                    "  {:<12}{:<7}{:<7}{:<9}{}",
                    k.tag(),
                    tiles(k),
                    m.tag(),
                    ph.tag(),
                    "native"
                );
            }
            continue;
        }
        // A component whose frame is the only thing wrong with it is not an
        // import at all, it is a design decision -- which is the difference
        // between "1890 cannot have a crusher" and "1890's crushers are on cast
        // iron". Saying so here is the whole reason this column exists.
        let offered = parts::phys(k).mats;
        let other: Vec<&str> = MATS
            .iter()
            .filter(|o| **o != m && offered.contains(o) && ph.builds_on(k, **o))
            .map(|o| o.tag())
            .collect();
        let from = phase::made_in(k, m);
        println!(
            "  {:<12}{:<7}{:<7}{:<9}{:<9}{}",
            k.tag(),
            tiles(k),
            m.tag(),
            from.map(|f| f.tag()).unwrap_or("nowhere"),
            if phase::crateable(k) { commas(cost) } else { "-".to_string() },
            if !other.is_empty() {
                format!("native on {}", other.join(" or "))
            } else if phase::crateable(k) {
                String::new()
            } else {
                "no crate of one would help".to_string()
            }
        );
    }
    if part.is_none() {
        println!("\n  and {free} components {} can simply build.", ph.tag());
        rule("the two designs this experiment added");
        for (file, src) in [("23-watermill", play::WATERMILL), ("24-hydro", play::HYDRO)] {
            let Ok(d) = Design::parse(src) else { continue };
            let cost = phase::design_cost(&d, ph);
            println!(
                "\n  \x1b[1m{}\x1b[0m ({file}.machine) in {}: {} gears",
                d.name,
                ph.tag(),
                commas(cost)
            );
            for (unit, kind, c) in phase::imported(&d, ph) {
                println!("    {:<6}{:<14}{:>8} gears", unit, kind.tag(), commas(c));
            }
            if cost == 0 {
                println!("    every component of it is native");
            }
        }
    }
    0
}

/// Four digits on a dark plate, in a 3x5 font, because the alternative was to
/// reach into experiment 08's rasteriser for a glyph table it keeps private.
fn year(buf: &mut [u8], stride: usize, x0: usize, y0: usize, text: &str, s: usize) {
    const DIGITS: [[u8; 5]; 10] = [
        [0b111, 0b101, 0b101, 0b101, 0b111],
        [0b010, 0b110, 0b010, 0b010, 0b111],
        [0b111, 0b001, 0b111, 0b100, 0b111],
        [0b111, 0b001, 0b111, 0b001, 0b111],
        [0b101, 0b101, 0b111, 0b001, 0b001],
        [0b111, 0b100, 0b111, 0b001, 0b111],
        [0b111, 0b100, 0b111, 0b101, 0b111],
        [0b111, 0b001, 0b001, 0b001, 0b001],
        [0b111, 0b101, 0b111, 0b101, 0b111],
        [0b111, 0b101, 0b111, 0b001, 0b111],
    ];
    let cols = text.chars().count();
    let (pw, ph) = (cols * 4 * s + 2 * s, 5 * s + 2 * s);
    let mut put = |x: usize, y: usize, c: [u8; 3]| {
        let i = (y * stride + x) * 3;
        if i + 3 <= buf.len() {
            buf[i..i + 3].copy_from_slice(&c);
        }
    };
    for y in 0..ph {
        for x in 0..pw {
            put(x0 + x, y0 + y, [22, 22, 26]);
        }
    }
    for (n, ch) in text.chars().enumerate() {
        let Some(d) = ch.to_digit(10) else { continue };
        for (ry, bits) in DIGITS[d as usize].iter().enumerate() {
            for bx in 0..3 {
                if bits & (1 << (2 - bx)) == 0 {
                    continue;
                }
                for dy in 0..s {
                    for dx in 0..s {
                        put(x0 + s + n * 4 * s + bx * s + dx, y0 + s + ry * s + dy, [238, 238, 232]);
                    }
                }
            }
        }
    }
}

fn tiles(k: parts::Kind) -> String {
    let p = parts::part(k);
    format!("{}", p.w * p.h)
}

// =============================================================== the refusals

/// Everything the slice will not allow, demonstrated rather than described.
fn refusals() -> i32 {
    use temporal_rooms::mp::cmd::Act;
    let mut s = Slice::open(7);
    s.start_manual();
    let ada = match s.join("Ada") {
        Ok(id) => id,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    rule("what the slice refuses, and the sentence it refuses with");
    let kestrel = land::feature("kestrel").map(|f| (f.x, f.y)).unwrap_or((8, 6));
    let cases: Vec<(&str, &str, Act)> = vec![
        (
            "district",
            "a bay where somebody's foundry is",
            Act::PlaceStorage { proto: "bay".into(), x: kestrel.0, y: kestrel.1, face: 0 },
        ),
        (
            "valley",
            "a powder line off a grid that does not exist",
            Act::PlaceMachine {
                proto: "powderline".into(),
                x: 52,
                y: 58,
                face: 0,
                item: None,
                design: temporal_rooms::mp::world::stock_design("powderline").ok(),
            },
        ),
        (
            "valley",
            "a hydro station with no crates landed",
            Act::PlaceMachine {
                proto: "turbinehall".into(),
                x: 52,
                y: 58,
                face: 0,
                item: None,
                design: Design::parse(play::HYDRO).ok(),
            },
        ),
        (
            "valley",
            "bulldozing the ore dock the region came with",
            // The first fixture of the first region: five deposits take ids
            // 1..5 out of the same counter, so the ore depot is 6.
            Act::DeleteMachine { id: 6 },
        ),
        (
            "district",
            "an arrival a player invented",
            Act::Deliver { to: 1, item: "IronOre".into(), qty: 99, from: "valley".into() },
        ),
    ];
    for (region, what, act) in cases {
        match s.submit(ada, region, act) {
            Ok(_) => println!("  \x1b[31m{what} was ALLOWED\x1b[0m"),
            Err(e) => {
                println!("\n  \x1b[1m{what}\x1b[0m  ({region})");
                println!("    {}", wrap(&e, 72, "    "));
            }
        }
    }

    rule("and the ones about logistics");
    let pairs: Vec<(&str, Box<dyn Fn(&mut Slice) -> Result<String, String>>)> = vec![
        (
            "a train fielded in 1890",
            Box::new(|s: &mut Slice| {
                s.open_route(1, "valley", "district", "IronOre", "train", None).map(|_| String::new())
            }),
        ),
        (
            "a run across a fracture nothing carries",
            Box::new(|s: &mut Slice| {
                s.open_route(1, "valley", "zone", "IronOre", "wagon", None).map(|_| String::new())
            }),
        ),
        (
            "a second interface on the corridor somebody already built",
            Box::new(|s: &mut Slice| s.open_gate(1, "near").map(|_| String::new())),
        ),
    ];
    for (what, f) in pairs {
        match f(&mut s) {
            Ok(_) => println!("  \x1b[31m{what} was ALLOWED\x1b[0m"),
            Err(e) => {
                println!("\n  \x1b[1m{what}\x1b[0m");
                println!("    {}", wrap(&e, 72, "    "));
            }
        }
    }

    rule("what it does not refuse");
    println!("  Nothing in this experiment refuses a *component* for being from the wrong");
    println!("  century. A motor in 1890 is a price, a grid connection in 1890 is a wire");
    println!("  with nothing on the end of it, and those are different answers on purpose.");
    let m = parts::by_tag("motor").expect("a motor");
    println!(
        "\n  a motor in 1890:      {} gears, out of {}",
        commas(phase::import_cost(m, Tune::default_for(m).mat, Phase::P1890)),
        phase::made_in(m, Tune::default_for(m).mat).map(|p| p.tag()).unwrap_or("nowhere")
    );
    let l = parts::by_tag("lathe").expect("a lathe");
    println!(
        "  a lathe in 1890:      {} gears, out of {} -- two fractures away",
        commas(phase::import_cost(l, Tune::default_for(l).mat, Phase::P1890)),
        phase::made_in(l, Tune::default_for(l).mat).map(|p| p.tag()).unwrap_or("nowhere")
    );
    0
}

// ==================================================================== output

fn yes(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// Wrap a sentence at `w` columns, indenting continuations.
fn wrap(s: &str, w: usize, pad: &str) -> String {
    let mut out = String::new();
    let mut line = 0;
    for word in s.split_whitespace() {
        if line > 0 && line + 1 + word.len() > w {
            out.push('\n');
            out.push_str(pad);
            line = 0;
        } else if line > 0 {
            out.push(' ');
            line += 1;
        }
        out.push_str(word);
        line += word.len();
    }
    out
}
