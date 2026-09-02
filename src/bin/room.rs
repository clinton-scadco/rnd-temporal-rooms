//! Prototype 2, from a terminal.
//!
//! ```text
//!   room serve [--port N]     the game, in two browsers
//!   room test  [--seed N]     the primary multiplayer test, played headlessly
//!   room fail                 the failure tests: nine ways to be ambiguous
//!   room goals [--seed N]     the templates, and what a seed makes of them
//!   room parts                the catalogue, and the recipes it compiles to
//! ```
//!
//! `test` is this experiment's acceptance command, in the same sense that
//! `machine space` is experiment 10's. Section 25 of the brief writes out one
//! scenario -- host, goal, late join, build at two scales, redesign, delete,
//! rebuild -- and asks for the three replicas to be compared at several probe
//! ticks. Running that in a browser proves it for the afternoon somebody ran
//! it. Running it here proves it every time anybody types `cargo test`, with
//! the clock held still so that a slow machine is not a different experiment
//! from a fast one.

use temporal_rooms::mp::cmd::Act;
use temporal_rooms::mp::goal::{Goal, TEMPLATES};
use temporal_rooms::mp::kit::{Role, PROTOS};
use temporal_rooms::mp::room::Room;
use temporal_rooms::mp::world::{stock_design, Id};
use temporal_rooms::model::Tick;
use temporal_rooms::mp::{as_secs, lower, net, secs};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("serve");
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    let code = match cmd {
        "serve" => {
            let port = flag(rest, "--port").and_then(|s| s.parse().ok()).unwrap_or(8790);
            match net::serve(port) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("cannot serve on port {port}: {e}");
                    1
                }
            }
        }
        "test" => scenario(flag(rest, "--seed").and_then(|s| s.parse().ok()).unwrap_or(7)),
        "fail" => failures(),
        "goals" => goals(rest),
        "parts" => parts(),
        other => {
            eprintln!("`{other}` is not a command. Try `serve`, `test`, `fail`, `goals` or `parts`.");
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

// ==================================================================== test

/// Section 25, played out with the clock in a vice.
fn scenario(seed: u64) -> i32 {
    let mut r = Room::open(seed, Some("first-gears"));
    rule(&format!("room {} -- {}", r.code, r.goal.title));
    println!("{}", r.goal.brief());
    println!("{}\n", r.goal.note);
    r.start_manual();

    let a = match r.join("Ada") {
        Ok(id) => id,
        Err(e) => return fail(&e),
    };
    println!("  t+0s     Ada hosts. The clock is running and will not stop.");

    // ---- the starting plot, and what is where
    let named = |r: &Room, tag: &str, n: usize| -> Id {
        r.host
            .world
            .installs
            .iter()
            .filter(|i| i.proto.tag == tag)
            .nth(n)
            .map(|i| i.id)
            .unwrap_or(0)
    };
    let depot = named(&r, "depot", 0);
    // A room comes with ground now, not with working mines, so the first thing
    // anybody does is put a head on each seam.
    let head = |r: &mut Room, who: u32, tag: &str| -> Id {
        let item = temporal_rooms::mp::kit::proto(tag).and_then(|p| p.extracts()).unwrap_or("");
        let Some((x, y)) = r.host.world.nth_ground(item, 0).map(|d| (d.x, d.y)) else {
            return 0;
        };
        let act =
            Act::PlaceMachine { proto: tag.into(), x, y, face: 0, item: None, design: None, example: true };
        match r.submit(who, act) {
            Ok(_) => r.host.world.installs.last().map(|i| i.id).unwrap_or(0),
            Err(_) => 0,
        }
    };
    let caster = head(&mut r, a, "billetcaster");
    let coal = head(&mut r, a, "coalpit");
    let water = head(&mut r, a, "waterpump");
    let bays: Vec<Id> =
        r.host.world.installs.iter().filter(|i| i.proto.tag == "bay").map(|i| i.id).collect();

    let step = |r: &mut Room, at: u64, who: u32, what: &str, act: Act| -> Id {
        r.set_now(secs(at));
        match r.submit(who, act) {
            Ok(_) => {
                let id = r.host.world.installs.last().map(|i| i.id).unwrap_or(0);
                println!("  {:<9}{what}", format!("t+{at}s"));
                id
            }
            Err(e) => {
                println!("  {:<9}{what}  \x1b[31mREFUSED: {e}\x1b[0m", format!("t+{at}s"));
                0
            }
        }
    };

    // ---- A starts a machine, alone
    let press = step(
        &mut r,
        4,
        a,
        "Ada places a stamping line",
        Act::PlaceMachine {
            proto: "stamping".into(),
            x: 40,
            y: 6,
            face: 0,
            item: None,
            design: None,
            example: true,
        },
    );
    let gearbay = step(
        &mut r,
        6,
        a,
        "Ada places a bay for the gears",
        Act::PlaceStorage { proto: "bay".into(), x: 60, y: 6, face: 0 },
    );

    // ---- B joins an already-running room
    r.set_now(secs(12));
    let b = match r.join("Bee") {
        Ok(id) => id,
        Err(e) => return fail(&e),
    };
    println!(
        "  t+12s    Bee joins with the code. The host did not pause: snapshot @ tick {}, \
         then the command stream.",
        r.player(b).map(|p| p.joined).unwrap_or(0)
    );

    // ---- B builds the world logistics while A is inside a machine
    let wire = |r: &mut Room, at: u64, who: u32, from: Id, to: Id, item: &str, what: &str| {
        r.set_now(secs(at));
        match r.submit(who, Act::CreateConnection { from, to, item: item.into() }) {
            Ok(_) => println!("  {:<9}{what}", format!("t+{at}s")),
            Err(e) => {
                println!("  {:<9}{what}  \x1b[31mREFUSED: {e}\x1b[0m", format!("t+{at}s"))
            }
        }
    };
    wire(&mut r, 14, b, caster, bays[0], "IronBillet", "Bee wires the caster to its bay");
    wire(&mut r, 14, b, coal, bays[1], "Coal", "Bee wires the coal pit to its bay");
    wire(&mut r, 14, b, water, bays[2], "Water", "Bee wires the intake to its bay");

    let far = step(
        &mut r,
        16,
        b,
        "Bee puts a bay next to the press",
        Act::PlaceStorage { proto: "bay".into(), x: 30, y: 6, face: 0 },
    );
    r.set_now(secs(18));
    match r.submit(
        b,
        Act::CreateWorldLink {
            proto: "belt".into(),
            from: bays[0],
            to: far,
            item: "IronBillet".into(),
        },
    ) {
        Ok(_) => {
            let h = r.host.world.hauls.last().unwrap();
            let d = r.host.world.span(h);
            println!(
                "  t+18s    Bee runs a belt across the plot: {} tenths of a tile, \
                 {:.1}s each way -- derived, not typed",
                d / 100,
                as_secs(match h.proto.spec {
                    temporal_rooms::mp::kit::Spec::Transport { speed, base, .. } =>
                        base + d / speed.max(1),
                    _ => 0,
                })
            );
        }
        Err(e) => println!("  t+18s    the belt was refused: {e}"),
    }
    wire(&mut r, 20, b, far, press, "IronBillet", "Bee feeds the press from that bay");
    wire(&mut r, 20, b, bays[1], press, "Coal", "Bee gives the press its coal");
    wire(&mut r, 20, b, press, gearbay, "Gear", "Bee sends the gears to a bay");
    wire(&mut r, 20, b, gearbay, depot, "Gear", "Bee sends that bay to the depot");

    let plant = step(
        &mut r,
        24,
        a,
        "Ada places a compact steam plant",
        Act::PlaceMachine {
            proto: "steamplant".into(),
            x: 40,
            y: 26,
            face: 0,
            item: None,
            design: None,
            example: true,
        },
    );
    let powerbay = step(
        &mut r,
        26,
        a,
        "Ada places a yard for the electricity",
        Act::PlaceStorage { proto: "yard".into(), x: 60, y: 26, face: 0 },
    );
    wire(&mut r, 28, a, bays[1], plant, "Coal", "Ada feeds the plant coal");
    wire(&mut r, 28, a, bays[2], plant, "Water", "Ada feeds the plant water");
    wire(&mut r, 28, a, plant, powerbay, "Power", "Ada wires the plant to the yard");
    wire(&mut r, 28, a, powerbay, press, "Power", "Ada powers the press");

    // One plant is 108 MW and the press wants 121, so the first thing the
    // factory teaches anybody is that it needs a second one.
    let plant2 = step(
        &mut r,
        34,
        a,
        "Ada finds the press starved of power and builds a second plant",
        Act::PlaceMachine {
            proto: "steamplant".into(),
            x: 40,
            y: 34,
            face: 0,
            item: None,
            design: None,
            example: true,
        },
    );
    wire(&mut r, 36, a, bays[1], plant2, "Coal", "Ada feeds it coal");
    wire(&mut r, 36, a, bays[2], plant2, "Water", "Ada feeds it water");
    wire(&mut r, 36, a, plant2, powerbay, "Power", "Ada wires it to the same yard");

    probe(&mut r, secs(60), a, b);

    // ---- A redesigns the press while it is running
    rule("Ada opens the press while it is making gears");
    r.set_now(secs(90));
    let before = r.host.world.get(press).and_then(|i| i.lowered.clone());
    if let Err(e) = r.submit(a, Act::OpenDesign { id: press }) {
        return fail(&e);
    }
    println!("  the live machine keeps its design, its population and its place in every queue.");
    match r.submit(b, Act::OpenDesign { id: press }) {
        Ok(_) => println!("  \x1b[31mBee opened the same draft -- the lock did not hold\x1b[0m"),
        Err(e) => println!("  Bee tries to open the same draft: refused -- {e}"),
    }
    for act in [
        Act::PlaceComponent {
            id: press,
            kind: "motor".into(),
            x: 7,
            y: 7,
            z: 0,
            face: None,
        },
        Act::ConnectComponent {
            id: press,
            from: "CB1".into(),
            from_port: "out".into(),
            to: "MO3".into(),
            to_port: "power".into(),
        },
        Act::ConnectComponent {
            id: press,
            from: "MO3".into(),
            from_port: "rotary".into(),
            to: "SH1".into(),
            to_port: "in".into(),
        },
    ] {
        let verb = act.verb();
        match r.submit(a, act) {
            Ok(_) => println!("  {verb} in the draft"),
            Err(e) => println!("  {verb} \x1b[31mrefused: {e}\x1b[0m"),
        }
    }
    let during = r.host.world.get(press).and_then(|i| i.lowered.clone());
    println!(
        "  the running machine is {}",
        if before == during { "untouched, as it should be" } else { "\x1b[31mchanged\x1b[0m" }
    );
    r.set_now(secs(100));
    let draft = r.host.world.get(press).and_then(|i| i.draft.clone());
    if let Some(d) = draft {
        match r.submit(a, Act::CommitMachineDesign { id: press, design: d }) {
            Ok(c) => println!(
                "  t+100s   CommitMachineDesign at tick {} seq {} -- one command, one tick",
                c.tick, c.seq
            ),
            Err(e) => println!("  the commit was refused: {e}"),
        }
    }
    if let (Some(was), Some(now)) =
        (before, r.host.world.get(press).and_then(|i| i.lowered.clone()))
    {
        println!(
            "  the press was {:?} every {:.0}s; it is now {:?} every {:.0}s",
            was.gives,
            as_secs(was.cycle),
            now.gives,
            as_secs(now.cycle)
        );
    }

    probe(&mut r, secs(150), a, b);

    // ---- somebody deletes and rebuilds something
    rule("Bee deletes a bay and puts it back");
    r.set_now(secs(160));
    match r.submit(b, Act::DeleteStorage { id: powerbay }) {
        Ok(_) => println!("  t+160s   the power bay is gone, and the press is unpowered."),
        Err(e) => println!("  the delete was refused: {e}"),
    }
    let ghost = r.host.ghosts.last().map(|g| (g.name.clone(), g.restore()));
    r.set_now(secs(166));
    if let Some((name, act)) = ghost {
        match r.submit(b, act) {
            Ok(_) => println!("  t+166s   restored from the ghost of {name} -- as a new placement, not a rollback"),
            Err(e) => println!("  the restore was refused: {e}"),
        }
        let new = r.host.world.installs.last().map(|i| i.id).unwrap_or(0);
        wire(&mut r, 168, b, plant, new, "Power", "Bee re-wires the plant to it");
        wire(&mut r, 168, b, new, press, "Power", "Bee re-powers the press");
    }

    // ---- run it out
    rule("the room runs on");
    for k in 1..=45 {
        r.set_now(secs(170 + k * 20));
        let _ = r.sync(a);
        if k % 2 == 0 {
            let _ = r.sync(b);
        }
    }
    let end = r.now();
    let ok = probe(&mut r, end, a, b);

    let p = r.host.progress();
    rule("the goal");
    for l in &p.lines {
        println!(
            "  {:<44} {:>12} / {:<12} {}",
            l.what,
            format!("{:.1}", l.have),
            format!("{:.1}", l.need),
            if l.met { "met" } else { "not yet" }
        );
    }
    match &p.done {
        Some(d) => println!(
            "\n  \x1b[32mGoal complete at {:.0}s\x1b[0m -- {} installations, {} of them machines, \
             {} tiles of plot.\n  The room did not stop.",
            as_secs(d.at),
            d.installs,
            d.machines,
            d.footprint
        ),
        None => println!(
            "\n  not finished in {:.0}s: {}",
            as_secs(r.now()),
            p.lines
                .iter()
                .filter(|l| !l.met)
                .map(|l| l.what.clone())
                .collect::<Vec<_>>()
                .join("; ")
        ),
    }

    let pa = r.player(a).unwrap();
    let pb = r.player(b).unwrap();
    println!(
        "\n  checks agreed: Ada {}, Bee {}   mismatches: {} and {}   resynchronisations: {} and {}",
        pa.agreed, pb.agreed, pa.mismatches, pb.mismatches, pa.resyncs, pb.resyncs
    );
    if ok && pa.mismatches == 0 && pb.mismatches == 0 {
        println!("\n  \x1b[32mhost == Ada == Bee at every probe.\x1b[0m");
        0
    } else {
        println!("\n  \x1b[31mthe replicas disagreed.\x1b[0m");
        1
    }
}

/// Compare the three reconstructions at one tick, and say so.
fn probe(r: &mut Room, at: u64, a: u32, b: u32) -> bool {
    r.set_now(at);
    let _ = r.sync(a);
    let _ = r.sync(b);
    let t = r
        .host
        .probe()
        .min(r.player(a).map(|p| p.sim.probe()).unwrap_or(0))
        .min(r.player(b).map(|p| p.sim.probe()).unwrap_or(0));
    let hs = r.hashes(t);
    let same = hs.iter().all(|(_, h)| h.is_some() && *h == hs[0].1);
    println!(
        "\n  probe @ {:.0}s   {}",
        as_secs(t),
        hs.iter()
            .map(|(who, h)| format!(
                "{who} {}",
                h.map(|h| format!("{h:016x}")).unwrap_or_else(|| "--".into())
            ))
            .collect::<Vec<_>>()
            .join("   ")
    );
    if !same {
        println!("  \x1b[31mthese are not the same room.\x1b[0m");
    }
    same
}

/// Whether one player's reconstruction and the host's agree at one tick.
fn agrees(r: &Room, who: u32, t: Tick) -> bool {
    match (r.host.check(t), r.player(who).and_then(|p| p.sim.check(t))) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn fail(e: &str) -> i32 {
    eprintln!("the scenario could not be set up: {e}");
    1
}

// ================================================================ failures

/// Section 26: nine ways for two players to be ambiguous, and what happens
/// instead.
fn failures() -> i32 {
    let mut bad = 0;
    let mut n = 0;
    let mut check = |what: &str, ok: bool, said: String| {
        n += 1;
        println!(
            "  {} {:<46} {}",
            if ok { "\x1b[32mok  \x1b[0m" } else { "\x1b[31mFAIL\x1b[0m" },
            what,
            said
        );
        if !ok {
            bad += 1;
        }
    };

    rule("failure tests");
    let mut r = Room::open(11, Some("first-gears"));
    r.start_manual();
    let a = r.join("Ada").unwrap();
    let b = r.join("Bee").unwrap();
    let bay = |r: &mut Room, who: u32, x: i32, y: i32| {
        r.submit(who, Act::PlaceStorage { proto: "bay".into(), x, y, face: 0 })
            .map(|_| r.host.world.installs.last().unwrap().id)
    };

    // 1. two commands at the same tick
    r.set_now(secs(5));
    let one = bay(&mut r, a, 40, 40).unwrap();
    let _ = bay(&mut r, b, 50, 40).unwrap();
    let (x, y) = (r.log[r.log.len() - 2].clone(), r.log[r.log.len() - 1].clone());
    check(
        "two commands at the same tick",
        x.tick == y.tick && y.seq == x.seq + 1,
        format!("both at tick {}, ordered {} then {}", x.tick, x.seq, y.seq),
    );

    // 1b. and two that want the same tiles
    let clash = r.submit(b, Act::PlaceStorage { proto: "bay".into(), x: 41, y: 41, face: 0 });
    check(
        "two players placing on the same tiles",
        clash.is_err(),
        clash.err().unwrap_or_default(),
    );

    // 2. rapid place and delete
    r.set_now(secs(6));
    let quick = bay(&mut r, a, 60, 40).unwrap();
    let del = r.submit(a, Act::DeleteStorage { id: quick });
    let ghosted = r.host.ghosts.iter().any(|g| g.name.ends_with(&quick.to_string()));
    check(
        "place and delete in the same instant",
        del.is_ok() && ghosted,
        "the bay existed for zero ticks and left a ghost anyway".into(),
    );

    // 3. an invalid connection
    let e = r.submit(a, Act::CreateConnection { from: one, to: one, item: "Coal".into() });
    check(
        "a bay wired to itself",
        e.is_err(),
        e.err().unwrap_or_default(),
    );

    // 4. a machine another player is editing
    r.set_now(secs(8));
    let press = r
        .submit(
            a,
            Act::PlaceMachine {
                proto: "stamping".into(),
                x: 70,
                y: 60,
                face: 0,
                item: None,
                design: None,
                example: true,
            },
        )
        .map(|_| r.host.world.installs.last().unwrap().id)
        .unwrap();
    r.submit(a, Act::OpenDesign { id: press }).unwrap();
    let e = r.submit(b, Act::OpenDesign { id: press });
    check("two players in one draft", e.is_err(), e.err().unwrap_or_default());
    let e = r.submit(b, Act::DeleteMachine { id: press });
    check(
        "deleting a machine somebody is editing",
        e.is_err(),
        e.err().unwrap_or_default(),
    );

    // 5. joining while a draft is open
    r.set_now(secs(10));
    let c = r.join("Cy").unwrap();
    let _ = r.sync(c);
    let sees_draft = r
        .player(c)
        .map(|p| p.sim.world.get(press).is_some_and(|i| i.draft.is_some() && i.editor == Some(a)))
        .unwrap_or(false);
    check(
        "joining while a machine is being edited",
        sees_draft,
        "the joiner is handed the draft and the lock along with the world".into(),
    );

    // 6. committing while the world changes underneath
    r.set_now(secs(12));
    let _ = r.submit(
        a,
        Act::PlaceComponent { id: press, kind: "motor".into(), x: 7, y: 7, z: 0, face: None },
    );
    let _ = bay(&mut r, b, 20, 20);
    let draft = r.host.world.get(press).and_then(|i| i.draft.clone()).unwrap();
    let committed = r.submit(a, Act::CommitMachineDesign { id: press, design: draft });
    check(
        "committing a design while the world changes",
        committed.is_ok(),
        format!("one command at tick {}", committed.map(|c| c.tick).unwrap_or(0)),
    );

    // 7. a late command is stamped by the host, not by the client
    r.set_now(secs(20));
    let late = r.submit(b, Act::DeleteStorage { id: one }).unwrap();
    check(
        "a command that arrives late",
        late.tick == secs(20),
        format!("stamped at tick {} -- the host's clock, never the client's", late.tick),
    );

    // 8. disconnect and reconnect
    r.set_now(secs(30));
    let _ = r.sync(a);
    let rejoined = r.join("Ada again").unwrap();
    r.set_now(secs(34));
    let _ = r.sync(rejoined);
    let t = r.host.probe().min(r.player(rejoined).map(|p| p.sim.probe()).unwrap_or(0));
    let same = agrees(&r, rejoined, t);
    check(
        "leaving and coming back",
        same,
        format!("rebuilt from a snapshot and agrees at {:.0}s", as_secs(t)),
    );

    // 9. a client that has diverged
    r.set_now(secs(40));
    let _ = r.sync(b);
    if let Some(p) = r.players.iter_mut().find(|p| p.id == b) {
        // Reach in and corrupt one replica, which is the only way to see the
        // correction path without waiting for a bug.
        let _ = p.sim.world.place(
            temporal_rooms::mp::kit::proto("bay").unwrap(),
            100,
            100,
            0,
            None,
            None,
            0,
            0,
        );
    }
    r.set_now(secs(46));
    let _ = r.sync(b);
    r.set_now(secs(52));
    let _ = r.sync(b);
    let p = r.player(b).unwrap();
    let t = r.host.probe().min(p.sim.probe());
    let healed = agrees(&r, b, t);
    check(
        "a client whose hash does not match",
        p.mismatches > 0 && p.resyncs > 0 && healed,
        format!(
            "{} mismatch, {} snapshot resent, agreeing again at {:.0}s",
            p.mismatches,
            p.resyncs,
            as_secs(t)
        ),
    );

    println!();
    if bad == 0 {
        println!(
            "  \x1b[32mall {n} resolved deterministically or were refused outright.\x1b[0m"
        );
        0
    } else {
        println!("  \x1b[31m{bad} of {n} did not.\x1b[0m");
        1
    }
}

// =================================================================== goals

fn goals(args: &[String]) -> i32 {
    if let Some(seed) = flag(args, "--seed").and_then(|s| s.parse::<u64>().ok()) {
        let g = Goal::of_seed(seed, flag(args, "--template"));
        rule(&format!("seed {seed}"));
        println!("  {} ({})", g.title, g.family.word());
        println!("  {}", g.brief());
        println!("  {}", g.note);
        println!("\n  it starts you with:");
        for (tag, item) in g.starting_kit() {
            println!(
                "    {:<14} {}",
                tag,
                item.unwrap_or_default()
            );
        }
        return 0;
    }
    rule(&format!("{} goal templates", TEMPLATES.len()));
    for t in TEMPLATES {
        println!("  {:<12} {:<26} {}", t.family.word(), t.id, t.title);
        println!("               {}", Goal::of_seed(1, Some(t.id)).brief());
    }
    println!("\n  Every one of them is written by hand. The seed chooses among them and");
    println!("  picks numbers inside ranges each template declares, and nothing else.");
    0
}

// =================================================================== parts

/// The catalogue, and -- for the machines -- the recipe their design compiles
/// to. Nobody typed those numbers; they are what the orbit does.
fn parts() -> i32 {
    rule("what can be placed");
    for p in PROTOS {
        println!(
            "  {:<14} {:<22} {:<10} {:>3}x{:<3} {}",
            p.tag,
            p.title,
            p.role.word(),
            p.w,
            p.h,
            p.rate_note()
        );
    }
    rule("what the machines compile to");
    for p in PROTOS.iter().filter(|p| p.role == Role::Machine) {
        let d = match stock_design(p.tag) {
            Ok(d) => d,
            Err(e) => {
                println!("  {:<14} \x1b[31m{e}\x1b[0m", p.tag);
                continue;
            }
        };
        match lower::lower(&d) {
            Ok(m) => {
                let list = |v: &[(String, u64)]| {
                    v.iter()
                        .map(|(i, q)| format!("{q} {i}"))
                        .collect::<Vec<_>>()
                        .join(" + ")
                };
                println!(
                    "  {:<14} {:>5.1}s  {:<44} -> {}",
                    p.tag,
                    as_secs(m.cycle),
                    list(&m.takes),
                    list(&m.gives)
                );
                println!(
                    "                 {:>4}x{:<3} tiles, {} components, orbit {}",
                    m.w,
                    m.h,
                    m.components,
                    if m.settled { "exact" } else { "unsettled" }
                );
            }
            Err(e) => println!("  {:<14} \x1b[31m{e}\x1b[0m", p.tag),
        }
    }
    0
}
