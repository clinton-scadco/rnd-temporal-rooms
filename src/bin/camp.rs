//! Prototype 3, from a terminal.
//!
//! ```text
//!   camp serve [--port N] [--host ADDR]   five rooms, in two browsers
//!   camp play  [--seed N]   the whole campaign, played headlessly
//!   camp map                the five rooms, the seven lanes, the three fleets
//!   camp tech               the twelve components, and what each one opens
//!   camp refuse             the things the campaign will not let you do
//! ```
//!
//! `play` is this experiment's acceptance command, in the same sense that
//! `room test` is Prototype 2's. It plays all five rooms end to end with the
//! clock held still -- building, opening supply lines, saving designs to the
//! shelf, going back to Iron Valley once the separator arrives -- and then
//! asks the three questions the prototype exists to answer:
//!
//! ```text
//!   did every room finish, in the order the map says?
//!   did the rooms nobody was standing in keep running?
//!   did every replica of every room agree with its host, all the way through?
//! ```
//!
//! It is a *good* run rather than an optimal one: everything it does is
//! something a player could do with the catalogue and the mouse, and none of
//! it needs a design nobody has written yet. The point is not the score. The
//! point is that the loop closes.

use temporal_rooms::camp::play::{self, clock, Play};
use temporal_rooms::camp::run::Camp;
use temporal_rooms::camp::{ship, site, tech};
use temporal_rooms::mp::cmd::Act;
use temporal_rooms::mp::goal::commas;
use temporal_rooms::mp::{as_secs, secs};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("serve");
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    let code = match cmd {
        "serve" => {
            let port = flag(rest, "--port").and_then(|s| s.parse().ok()).unwrap_or(8795);
            // Loopback by default, because a campaign left running is not a
            // thing to put on the network without saying so. `--host 0.0.0.0`
            // is how you say so, and it is what a second machine needs.
            let host = flag(rest, "--host").unwrap_or("127.0.0.1");
            match temporal_rooms::camp::net::serve(host, port) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("cannot serve on {host} port {port}: {e}");
                    1
                }
            }
        }
        "play" => play(flag(rest, "--seed").and_then(|s| s.parse().ok()).unwrap_or(3)),
        "map" => map(),
        "tech" => techs(),
        "refuse" => refusals(),
        other => {
            eprintln!("`{other}` is not a command. Try `serve`, `play`, `map`, `tech` or `refuse`.");
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

/// The whole campaign, played headlessly, with a running commentary.
///
/// The script itself is [`camp::play`], in the library, because `tests/camp.rs`
/// asserts on the same run: a playthrough only the binary could run would be a
/// demonstration rather than a proof. What is left here is the printing.
fn play(seed: u64) -> i32 {
    let mut p = Play::open(seed);
    play::run(&mut p);
    finish(p)
}

/// What happened, and whether any of it was wrong.
fn finish(mut p: Play) -> i32 {
    rule("the campaign");
    for s in site::SITES {
        let at = p.c.done.get(s.tag).copied();
        let y = p.c.yard(s.tag).expect("a room of the campaign");
        println!(
            "  {:<10}{:<16} {:>8}   {:>3} installations, {:>2} machines, {} tiles",
            s.tag,
            match at {
                Some(t) => clock(t),
                None => "--".into(),
            },
            if at.is_some() { "done" } else { "open" },
            y.room.host.world.installs.len(),
            y.room
                .host
                .world
                .installs
                .iter()
                .filter(|i| i.proto.role == temporal_rooms::mp::kit::Role::Machine)
                .count(),
            commas(y.room.host.world.footprint() as u64),
        );
    }

    rule("what moved between the rooms");
    for r in &p.c.ledger.routes {
        let l = r.lane();
        println!(
            "  {:<8}-> {:<8}{:<14}{:<8}{:>10} moved in {:>3} trips, {} spilled",
            l.from,
            l.to,
            l.item,
            r.fleet.tag,
            commas(r.moved),
            r.trips,
            commas(r.spilled),
        );
    }

    rule("the shelf");
    for s in &p.c.shelf.items {
        println!(
            "  {:<26}{}{}",
            s.name,
            s.note(),
            match s.from.and_then(|f| p.c.shelf.get(f)) {
                Some(parent) => format!("   (from {})", parent.name),
                None => String::new(),
            }
        );
    }

    rule("what was unlocked");
    for u in p.c.tech.earned() {
        println!("  {:<26}{}", u.title, u.opens);
    }
    println!("  {} of {} components", p.c.tech.earned().len(), tech::UNLOCKS.len());

    rule("the proof");
    let now = p.c.now();
    println!("  simulated             {}", clock(now));
    println!("  rooms finished        {} of {}", p.c.done.len(), site::SITES.len());
    println!("  hash comparisons      {}", p.checks);
    for (tag, hs) in p.c.hashes(ship::settled(now).saturating_sub(secs(5))) {
        let all: Vec<String> = hs
            .iter()
            .map(|(who, h)| match h {
                Some(h) => format!("{who} {h:016x}"),
                None => format!("{who} --"),
            })
            .collect();
        let agree = hs.iter().filter_map(|(_, h)| *h).collect::<Vec<_>>();
        let same = agree.windows(2).all(|w| w[0] == w[1]);
        println!(
            "  {:<10}{}  {}",
            tag,
            if same { "\x1b[32magree\x1b[0m" } else { "\x1b[31mDIFFER\x1b[0m" },
            all.join("   ")
        );
        if !same {
            p.bad.push(format!("{tag}: the replicas do not agree"));
        }
    }

    if p.bad.is_empty() && p.c.complete() {
        println!("\n\x1b[32mall five rooms finished, and every replica agreed throughout.\x1b[0m");
        0
    } else {
        if !p.c.complete() {
            println!("\n\x1b[31mthe campaign did not finish.\x1b[0m");
        }
        for b in &p.bad {
            println!("  \x1b[31m{b}\x1b[0m");
        }
        1
    }
}

// =================================================================== the map

fn map() -> i32 {
    rule("the five rooms");
    for s in site::SITES {
        println!("\n  \x1b[1m{}\x1b[0m  ({}, {} x {} tiles)", s.title, s.tag, s.plot, s.plot);
        println!("    {}", s.problem);
        println!("    objective   {}", goal_of(s));
        println!(
            "    opens after {}",
            if s.needs.is_empty() { "nothing -- it is where you start".to_string() } else { s.needs.join(", ") }
        );
        println!(
            "    hands over  {}",
            s.unlocks().iter().map(|u| u.title).collect::<Vec<_>>().join(", ")
        );
    }

    rule("the lanes");
    for l in ship::LANES {
        let times: Vec<String> = ship::FLEETS
            .iter()
            .map(|f| format!("{} {}s", f.tag, as_secs(f.trip(l)) as u64))
            .collect();
        println!(
            "  {:<8}-> {:<8}{:<14}{}",
            l.from,
            l.to,
            l.item,
            times.join("   ")
        );
        println!("           {}", l.why);
    }

    rule("the fleets");
    for f in ship::FLEETS {
        println!(
            "  {:<8}{:>9} x {}   {}s loading, speed {}",
            f.tag,
            commas(f.load),
            f.vehicles,
            f.dwell,
            f.speed
        );
        println!("           {}", f.blurb);
    }
    0
}

fn goal_of(s: &site::Site) -> String {
    temporal_rooms::mp::goal::Goal::of_seed(1, Some(s.template)).brief()
}

fn techs() -> i32 {
    rule("what a campaign starts with");
    let start = tech::starting();
    println!("  {} of the {} components:", start.len(), start.len() + tech::UNLOCKS.len());
    for row in start.chunks(6) {
        println!("    {}", row.join("  "));
    }

    rule("the twelve");
    for u in tech::UNLOCKS {
        let from = site::SITES
            .iter()
            .find(|s| s.gives.contains(&u.part))
            .map(|s| s.title)
            .unwrap_or("nowhere");
        println!("  {:<12}{:<28}{}", u.part, u.title, from);
        println!("               {}", u.opens);
    }

    rule("and what they make placeable");
    let mut got = tech::Tech::new();
    for s in site::SITES {
        for p in temporal_rooms::mp::kit::PROTOS
            .iter()
            .filter(|p| p.role == temporal_rooms::mp::kit::Role::Machine)
        {
            if got.allows_proto(p.tag).is_ok() {
                continue;
            }
            let after: Vec<&'static str> = s.gives.to_vec();
            let mut trial = got.clone();
            for part in &after {
                trial.learn(part);
            }
            if trial.allows_proto(p.tag).is_ok() {
                println!("  after {:<10}{}", s.tag, p.title);
            }
        }
        for part in s.gives {
            got.learn(part);
        }
    }
    for p in temporal_rooms::mp::kit::PROTOS
        .iter()
        .filter(|p| p.role == temporal_rooms::mp::kit::Role::Machine)
    {
        if tech::Tech::new().allows_proto(p.tag).is_ok() {
            println!("  from the start  {}", p.title);
        }
    }
    0
}

// =============================================================== the refusals

/// Everything the campaign will not let you do, demonstrated rather than
/// promised.
fn refusals() -> i32 {
    let mut bad = 0;
    let mut c = Camp::open(11);
    c.start_manual();
    c.set_now(secs(1));
    let ada = c.join("Ada").expect("a player");

    rule("what the campaign refuses, and what it says");
    let mut check = |what: &str, r: Result<String, String>| match r {
        Err(e) => println!("  {what:<50}\x1b[32mrefused\x1b[0m  {e}"),
        Ok(_) => {
            println!("  {what:<50}\x1b[31mALLOWED\x1b[0m");
            bad += 1;
        }
    };

    check(
        "walking into a room that is not open",
        c.travel(ada, "final").map(|_| String::new()),
    );
    check(
        "placing a machine whose parts are locked",
        c.submit(
            ada,
            "basin",
            Act::PlaceMachine {
                proto: "stamping".into(),
                x: 20,
                y: 20,
                face: 0,
                item: None,
                design: temporal_rooms::mp::world::stock_design("stamping").ok(),
            },
        )
        .map(|_| String::new()),
    );
    check(
        "reaching for a locked component in the designer",
        c.submit(
            ada,
            "basin",
            Act::PlaceComponent {
                id: 1,
                kind: "press".into(),
                x: 0,
                y: 0,
                z: 0,
                face: None,
            },
        )
        .map(|_| String::new()),
    );
    let seam = c
        .yard("basin")
        .and_then(|y| y.ports.fixtures.first().copied())
        .unwrap_or(0);
    check(
        "bulldozing the coal seam the room came with",
        c.submit(ada, "basin", Act::DeleteMachine { id: seam }).map(|_| String::new()),
    );
    check(
        "issuing an arrival by hand",
        c.submit(
            ada,
            "basin",
            Act::Deliver { to: seam, item: "Coal".into(), qty: 1_000, from: "nowhere".into() },
        )
        .map(|_| String::new()),
    );
    check(
        "opening a lane the map does not have",
        c.open_route(ada, "basin", "basin", "Coal", "train", None).map(|_| String::new()),
    );
    check(
        "opening a lane into a room that is shut",
        c.open_route(ada, "basin", "valley", "Coal", "train", None).map(|_| String::new()),
    );
    check(
        "saving two designs under one name",
        (|| {
            let plant = temporal_rooms::mp::world::stock_design("steamplant")?;
            c.shelf.save("Mk1", "steamplant", plant.clone(), None, "basin", 0, ada)?;
            c.shelf.save("Mk1", "steamplant", plant, None, "basin", 0, ada)?;
            Ok(String::new())
        })(),
    );
    check(
        "copying a design that is not on the shelf",
        c.copy(ada, 99, "Mk2").map(|_| String::new()),
    );

    rule("and what it allows");
    let mut allow = |what: &str, r: Result<String, String>| match r {
        Ok(note) => println!("  {what:<50}\x1b[32mallowed\x1b[0m  {note}"),
        Err(e) => {
            println!("  {what:<50}\x1b[31mREFUSED\x1b[0m  {e}");
            bad += 1;
        }
    };
    allow(
        "copying a design that is on the shelf",
        c.copy(ada, 1, "Mk2").map(|id| format!("Mk2 is design {id}, and remembers Mk1")),
    );
    allow(
        "placing a machine every part of which is yours",
        c.submit(
            ada,
            "basin",
            Act::PlaceMachine {
                proto: "steamplant".into(),
                x: 20,
                y: 20,
                face: 0,
                item: None,
                design: temporal_rooms::mp::world::stock_design("steamplant").ok(),
            },
        )
        .map(|cmd| format!("stamped at tick {}, sequence {}", cmd.tick, cmd.seq)),
    );

    if bad == 0 {
        println!("\n\x1b[32mevery ambiguity has one answer, and it is the same one twice.\x1b[0m");
        0
    } else {
        println!("\n\x1b[31m{bad} of them did not behave.\x1b[0m");
        1
    }
}
