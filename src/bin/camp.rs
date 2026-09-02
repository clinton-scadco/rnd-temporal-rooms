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

use temporal_rooms::camp::run::Camp;
use temporal_rooms::camp::{ship, site, tech};
use temporal_rooms::model::Tick;
use temporal_rooms::mp::cmd::Act;
use temporal_rooms::mp::goal::commas;
use temporal_rooms::mp::world::{Id, PlayerId};
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

/// A campaign with a clock somebody else is turning, and a running commentary.
struct Play {
    c: Camp,
    /// Seconds, because everything in the script is written in them.
    t: u64,
    bad: Vec<String>,
    checks: u64,
}

impl Play {
    fn open(seed: u64) -> Play {
        let mut c = Camp::open(seed);
        c.start_manual();
        Play { c, t: 0, bad: Vec::new(), checks: 0 }
    }

    /// Move the clock to `s` seconds, never backwards, and let the campaign
    /// catch up -- which advances all five rooms, lands whatever was in the
    /// air, and loads whatever the depots have shipped.
    fn at(&mut self, s: u64) {
        self.t = self.t.max(s);
        self.c.set_now(secs(self.t));
        if let Err(e) = self.c.advance() {
            self.bad.push(format!("t+{}s: the campaign would not run: {e}", self.t));
        }
    }

    fn tick(&mut self, ds: u64) {
        let want = self.t + ds;
        self.at(want);
    }

    fn say(&self, what: &str) {
        println!("  {:<10}{what}", format!("t+{}s", self.t));
    }

    /// One intention, and the id of whatever it put down.
    fn act(&mut self, who: PlayerId, site: &str, what: &str, act: Act) -> Id {
        match self.c.submit(who, site, act) {
            Ok(_) => {
                let id = self
                    .c
                    .yard(site)
                    .and_then(|y| y.room.host.world.installs.last().map(|i| i.id))
                    .unwrap_or(0);
                if !what.is_empty() {
                    self.say(what);
                }
                id
            }
            Err(e) => {
                println!("  {:<10}{what}  \x1b[31mREFUSED: {e}\x1b[0m", format!("t+{}s", self.t));
                self.bad.push(format!("t+{}s: {what}: {e}", self.t));
                0
            }
        }
    }

    fn place(&mut self, who: PlayerId, site: &str, proto: &str, x: i32, y: i32) -> Id {
        let storage = matches!(proto, "bay" | "yard");
        let act = if storage {
            Act::PlaceStorage { proto: proto.into(), x, y, face: 0 }
        } else {
            // The playthrough places the catalogue's worked examples on
            // purpose. A player gets an empty chassis and designs it; a
            // harness that had to draw four steam plants out of parts would be
            // proving something about the designer rather than about the
            // campaign.
            Act::PlaceMachine {
                proto: proto.into(),
                x,
                y,
                face: 0,
                item: None,
                design: None,
                example: true,
            }
        };
        self.act(who, site, "", act)
    }

    /// A head on the n-th patch of ground of the kind this prototype works.
    ///
    /// Since experiment 13 a room comes with ground rather than with working
    /// mines, so the campaign has to build its own -- which is the point: the
    /// first thing this playthrough does in every room is decide how to get
    /// material out of it.
    fn head(&mut self, who: PlayerId, site: &str, proto: &str, n: usize) -> Id {
        let item = temporal_rooms::mp::kit::proto(proto).and_then(|p| p.extracts()).unwrap_or("");
        let at = self
            .c
            .yard(site)
            .and_then(|y| y.room.host.world.nth_ground(item, n))
            .map(|d| (d.x, d.y));
        match at {
            Some((x, y)) => self.place(who, site, proto, x, y),
            None => 0,
        }
    }

    /// Another head on the same ground, `dx` tiles along from its corner.
    ///
    /// A seam is a budget rather than a socket: what one head cannot lift, two
    /// or three standing beside each other can, for the price of the floor
    /// they take. In Coal Basin that price is the whole objective.
    fn beside(&mut self, who: PlayerId, site: &str, proto: &str, n: usize, dx: i32) -> Id {
        let item = temporal_rooms::mp::kit::proto(proto).and_then(|p| p.extracts()).unwrap_or("");
        let at = self
            .c
            .yard(site)
            .and_then(|y| y.room.host.world.nth_ground(item, n))
            .map(|d| (d.x + dx, d.y));
        match at {
            Some((x, y)) => self.place(who, site, proto, x, y),
            None => 0,
        }
    }

    fn wire(&mut self, who: PlayerId, site: &str, from: Id, to: Id, item: &str) {
        self.act(
            who,
            site,
            "",
            Act::CreateConnection { from, to, item: item.into() },
        );
    }

    /// Run until the room's objective is met, or give up and say so.
    fn until(&mut self, site: &str, tag: &str, cap: u64) -> bool {
        let start = self.t;
        while self.t < start + cap {
            self.tick(5);
            self.probe();
            if self.c.yard(site).and_then(|y| y.done_at()).is_some() {
                let at = self.c.yard(site).and_then(|y| y.done_at()).unwrap_or(0);
                println!(
                    "  {:<10}\x1b[32m{tag} met at {}\x1b[0m",
                    format!("t+{}s", self.t),
                    clock(at)
                );
                return true;
            }
        }
        println!("  {:<10}\x1b[31m{tag} did not finish inside {cap}s\x1b[0m", format!("t+{}s", self.t));
        self.report(site);
        self.bad.push(format!("{tag} never met its objective"));
        false
    }

    /// The comparison. Every replica of every room, against its host, at the
    /// newest lattice point they share.
    fn probe(&mut self) {
        for id in self.c.cast.iter().map(|c| c.id).collect::<Vec<_>>() {
            if let Err(e) = self.c.sync_all(id) {
                self.bad.push(format!("t+{}s: player {id} could not be synchronised: {e}", self.t));
            }
        }
        self.checks += 1;
        if !self.c.agrees() {
            self.bad.push(format!("t+{}s: a replica disagreed with its host", self.t));
        }
    }

    /// Why a room is not finished, in the room's own words.
    fn report(&mut self, site: &str) {
        let Some(y) = self.c.yard(site) else { return };
        for l in y.room.host.progress().lines {
            println!(
                "               {:<44} {:>12} / {} {}",
                l.what,
                fmt(l.have),
                fmt(l.need),
                l.unit
            );
        }
        for (id, why) in &y.room.host.build.idle {
            let name = y.room.host.world.get(*id).map(|i| i.name.clone()).unwrap_or_default();
            println!("               \x1b[33m{name} is idle: {why}\x1b[0m");
        }
    }
}

fn fmt(n: f64) -> String {
    if n >= 1000.0 {
        commas(n as u64)
    } else {
        format!("{n:.1}")
    }
}

fn clock(t: Tick) -> String {
    let s = t / 60;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The whole campaign, played headlessly.
fn play(seed: u64) -> i32 {
    let mut p = Play::open(seed);
    rule(&format!("campaign {} -- five rooms, one clock", p.c.code));
    for s in site::SITES {
        println!("  {:<10}{:<16} {}", s.tag, s.title, s.problem);
    }

    let ada = match p.c.join("Ada") {
        Ok(id) => id,
        Err(e) => return fail(&e),
    };
    let bruno = match p.c.join("Bruno") {
        Ok(id) => id,
        Err(e) => return fail(&e),
    };
    println!("\n  two players, five reconstructions each, compared every simulated second.\n");

    // ================================================== 1. Coal Basin
    rule("Coal Basin -- a platform too small for the plant it needs");
    p.at(4);
    p.say("Ada wires the seam, the intake and the grid");
    let fixture = |p: &Play, site: &str, tag: &str, n: usize| -> Id {
        p.c.yard(site)
            .and_then(|y| {
                y.room
                    .host
                    .world
                    .installs
                    .iter()
                    .filter(|i| i.proto.tag == tag)
                    .nth(n)
                    .map(|i| i.id)
            })
            .unwrap_or(0)
    };
    let grid = fixture(&p, "basin", "grid", 0);
    let coal_out = fixture(&p, "basin", "depot", 0);
    let seam = p.head(ada, "basin", "coalpit", 0);
    let intake = p.head(ada, "basin", "waterpump", 0);
    // The export seam is worth nine hundred a second and one head lifts four
    // hundred, so it takes three of them standing side by side -- which is the
    // trade this room is about, since every tile they cost is a tile the
    // plants wanted. The third one only gets the hundred that is left.
    let seam2 = p.head(ada, "basin", "coalpit", 1);
    let seam2b = p.beside(ada, "basin", "coalpit", 1, 2);
    let seam2c = p.beside(ada, "basin", "coalpit", 1, 4);

    let bay_c = p.place(ada, "basin", "bay", 8, 2);
    let bay_w = p.place(ada, "basin", "bay", 8, 8);
    let bay_p = p.place(ada, "basin", "bay", 24, 2);
    let bay_x = p.place(ada, "basin", "bay", 24, 8);
    p.wire(ada, "basin", seam, bay_c, "Coal");
    p.wire(ada, "basin", intake, bay_w, "Water");
    p.wire(ada, "basin", bay_p, grid, "Power");
    p.wire(ada, "basin", seam2, bay_x, "Coal");
    p.wire(ada, "basin", seam2b, bay_x, "Coal");
    p.wire(ada, "basin", seam2c, bay_x, "Coal");
    p.wire(ada, "basin", bay_x, coal_out, "Coal");

    p.at(10);
    p.say("four compact steam plants, packed into the middle");
    for k in 0..4 {
        let plant = p.place(ada, "basin", "steamplant", 14, 2 + k * 3);
        p.wire(ada, "basin", bay_c, plant, "Coal");
        p.wire(ada, "basin", bay_w, plant, "Water");
        p.wire(ada, "basin", plant, bay_p, "Power");
    }
    p.tick(2);
    let foot = p.c.yard("basin").map(|y| y.room.host.world.footprint()).unwrap_or(0);
    p.say(&format!("the whole plot fits in {foot} tiles"));

    // The shelf: the first design worth keeping, and the first copy of it.
    let first = p
        .c
        .yard("basin")
        .and_then(|y| {
            y.room
                .host
                .world
                .installs
                .iter()
                .find(|i| i.proto.tag == "steamplant")
                .map(|i| i.id)
        })
        .unwrap_or(0);
    match p.c.keep(ada, "basin", first, "Compact Steam Plant Mk1", false) {
        Ok(id) => {
            p.say("Ada puts the plant on the shelf as `Compact Steam Plant Mk1`");
            match p.c.copy(ada, id, "Low-Coal Mk1") {
                Ok(_) => p.say("and copies it to `Low-Coal Mk1` for later"),
                Err(e) => p.bad.push(format!("the shelf refused a copy: {e}")),
            }
        }
        Err(e) => p.bad.push(format!("the shelf refused a design: {e}")),
    }

    // The locked half of the catalogue, before anything has been earned.
    if p.c.submit(
        ada,
        "basin",
        Act::PlaceMachine {
            proto: "stamping".into(),
            x: 20,
            y: 20,
            face: 0,
            item: None,
            design: None,
            example: true,
        },
    )
    .is_ok()
    {
        p.bad.push("a stamping line was placeable before the press was unlocked".into());
    } else {
        p.say("a stamping line is refused: the press has not been unlocked");
    }
    if p.c.travel(bruno, "valley").is_ok() {
        p.bad.push("Iron Valley was open before Coal Basin was producing".into());
    } else {
        p.say("Iron Valley is shut: Coal Basin has to be producing first");
    }

    if !p.until("basin", "Coal Basin", 180) {
        return finish(p);
    }
    p.say(&format!(
        "unlocked: {}",
        p.c.tech.earned().iter().map(|u| u.title).collect::<Vec<_>>().join(", ")
    ));

    // ================================================== 2. Iron Valley
    rule("Iron Valley -- all the land in the world, and no fuel");
    if let Err(e) = p.c.travel(bruno, "valley") {
        return fail(&e);
    }
    p.say("Bruno walks to Iron Valley. Coal Basin keeps running behind him.");
    match p.c.open_route(ada, "basin", "valley", "Coal", "train", Some(200)) {
        Ok(_) => p.say("a train starts running coal, Basin to Valley: 50 seconds, 30,000 a load"),
        Err(e) => p.bad.push(format!("the coal line was refused: {e}")),
    }

    let mine1 = p.head(ada, "valley", "oremine", 0);
    let mine2 = p.head(ada, "valley", "oremine", 1);
    let seam_v = p.head(ada, "valley", "coalpit", 0);
    let water_v = p.head(ada, "valley", "waterpump", 0);
    let coal_in = fixture(&p, "valley", "yard", 0);
    let powder_out = fixture(&p, "valley", "depot", 0);

    p.at(p.t + 4);
    let bay_ore = p.place(bruno, "valley", "bay", 16, 6);
    let bay_coal_v = p.place(bruno, "valley", "bay", 16, 34);
    let bay_water_v = p.place(bruno, "valley", "bay", 16, 46);
    let bay_pow_v = p.place(bruno, "valley", "yard", 44, 40);
    let bay_powder = p.place(bruno, "valley", "bay", 84, 10);
    p.wire(bruno, "valley", mine1, bay_ore, "IronOre");
    p.wire(bruno, "valley", seam_v, bay_coal_v, "Coal");
    p.wire(bruno, "valley", water_v, bay_water_v, "Water");
    p.wire(bruno, "valley", bay_powder, powder_out, "OrePowder");
    p.say("bays for ore, the local seam, water, power and the product");

    p.at(p.t + 4);
    for (k, coal_from) in [coal_in, bay_coal_v].into_iter().enumerate() {
        let plant = p.place(bruno, "valley", "steamplant", 30, 50 + k as i32 * 4);
        p.wire(bruno, "valley", coal_from, plant, "Coal");
        p.wire(bruno, "valley", bay_water_v, plant, "Water");
        p.wire(bruno, "valley", plant, bay_pow_v, "Power");
    }
    p.say("two plants: one on the seam, one on whatever the train brings");

    let line = p.place(bruno, "valley", "powderline", 30, 10);
    p.wire(bruno, "valley", bay_ore, line, "IronOre");
    p.wire(bruno, "valley", bay_pow_v, line, "Power");
    p.wire(bruno, "valley", line, bay_powder, "OrePowder");
    p.say("a powder line: 135 ore and 270 MW every two seconds, for 135 powder");

    if !p.until("valley", "Iron Valley", 700) {
        return finish(p);
    }
    p.say(&format!(
        "unlocked: {}",
        p.c.yard("valley")
            .map(|y| y.site.unlocks().iter().map(|u| u.title).collect::<Vec<_>>().join(", "))
            .unwrap_or_default()
    ));

    // ================================================== 3. Power Station
    rule("Power Station -- every lump of coal a minute away");
    if let Err(e) = p.c.travel(ada, "station") {
        return fail(&e);
    }
    match p.c.open_route(ada, "basin", "station", "Coal", "train", Some(140)) {
        Ok(_) => p.say("a second train: Basin to the Station, 30,000 a load"),
        Err(e) => p.bad.push(format!("the station's coal line was refused: {e}")),
    }
    let water_s = p.head(ada, "station", "waterpump", 0);
    let coal_s = fixture(&p, "station", "yard", 0);
    let grid_s = fixture(&p, "station", "grid", 0);
    p.at(p.t + 4);
    let bay_ws = p.place(ada, "station", "bay", 20, 8);
    let bay_ps = p.place(ada, "station", "yard", 52, 8);
    p.wire(ada, "station", water_s, bay_ws, "Water");
    p.wire(ada, "station", bay_ps, grid_s, "Power");
    for k in 0..3 {
        let plant = p.place(ada, "station", "steamplant", 36, 8 + k * 4);
        p.wire(ada, "station", coal_s, plant, "Coal");
        p.wire(ada, "station", bay_ws, plant, "Water");
        p.wire(ada, "station", plant, bay_ps, "Power");
    }
    p.say("three plants, fed entirely by rail");
    if !p.until("station", "Power Station", 400) {
        return finish(p);
    }

    // ================================================== 4. Manufacturing
    rule("Manufacturing -- no coal, no water, no grid");
    if let Err(e) = p.c.travel(bruno, "works") {
        return fail(&e);
    }
    for (from, item, fleet, cap) in [
        ("basin", "Coal", "convoy", 30u64),
        ("station", "Power", "convoy", 200),
    ] {
        match p.c.open_route(bruno, from, "works", item, fleet, Some(cap)) {
            Ok(_) => p.say(&format!("{item} now runs {from} -> works by {fleet}")),
            Err(e) => p.bad.push(format!("the {item} line into works was refused: {e}")),
        }
    }
    let caster = p.head(ada, "works", "billetcaster", 0);
    let coal_w = fixture(&p, "works", "yard", 0);
    let pow_w = fixture(&p, "works", "yard", 1);
    let gear_out = fixture(&p, "works", "depot", 0);
    p.at(p.t + 4);
    let bay_bil = p.place(bruno, "works", "bay", 20, 8);
    let bay_gear = p.place(bruno, "works", "bay", 48, 12);
    p.wire(bruno, "works", caster, bay_bil, "IronBillet");
    p.wire(bruno, "works", bay_gear, gear_out, "Gear");
    let press = p.place(bruno, "works", "stamping", 32, 8);
    p.wire(bruno, "works", bay_bil, press, "IronBillet");
    p.wire(bruno, "works", coal_w, press, "Coal");
    p.wire(bruno, "works", pow_w, press, "Power");
    p.wire(bruno, "works", press, bay_gear, "Gear");
    p.say("a stamping line -- the press arrived with the Power Station");
    if !p.until("works", "Manufacturing", 500) {
        return finish(p);
    }

    // ============================ 5. back to Iron Valley, then Final Works
    rule("Iron Valley, again -- the separator changes what that room is for");
    if let Err(e) = p.c.travel(bruno, "valley") {
        return fail(&e);
    }
    p.say("Bruno goes back. The powder line has been running the whole time.");
    p.at(p.t + 4);
    let conc_out = fixture(&p, "valley", "depot", 1);
    let bay_ore2 = p.place(bruno, "valley", "bay", 16, 20);
    let bay_conc = p.place(bruno, "valley", "bay", 84, 24);
    p.wire(bruno, "valley", mine2, bay_ore2, "IronOre");
    p.wire(bruno, "valley", bay_conc, conc_out, "Concentrate");
    let crush = p.place(bruno, "valley", "crusher", 30, 20);
    p.wire(bruno, "valley", bay_ore2, crush, "IronOre");
    p.wire(bruno, "valley", coal_in, crush, "Coal");
    p.wire(bruno, "valley", bay_water_v, crush, "Water");
    p.wire(bruno, "valley", crush, bay_conc, "Concentrate");
    p.say("a steam crusher, which was not placeable an hour ago");

    rule("Final Works -- a load that will not sit still");
    if let Err(e) = p.c.travel(ada, "final") {
        return fail(&e);
    }
    for (from, item, fleet, cap) in [
        ("basin", "Coal", "train", 120u64),
        ("works", "Gear", "convoy", 60),
        ("valley", "Concentrate", "convoy", 60),
    ] {
        match p.c.open_route(ada, from, "final", item, fleet, Some(cap)) {
            Ok(_) => p.say(&format!("{item}: {from} -> Final Works")),
            Err(e) => p.bad.push(format!("the {item} line into Final Works was refused: {e}")),
        }
    }
    let water_f = p.head(ada, "final", "waterpump", 0);
    let coal_f = fixture(&p, "final", "yard", 0);
    let gear_in = fixture(&p, "final", "yard", 1);
    let conc_in = fixture(&p, "final", "yard", 2);
    let grid_f = fixture(&p, "final", "grid", 0);
    let gear_ship = fixture(&p, "final", "depot", 0);
    let conc_ship = fixture(&p, "final", "depot", 1);
    p.at(p.t + 4);
    let bay_wf = p.place(ada, "final", "bay", 20, 8);
    let bay_pf = p.place(ada, "final", "yard", 56, 8);
    p.wire(ada, "final", water_f, bay_wf, "Water");
    p.wire(ada, "final", bay_pf, grid_f, "Power");
    p.wire(ada, "final", gear_in, gear_ship, "Gear");
    p.wire(ada, "final", conc_in, conc_ship, "Concentrate");
    for k in 0..2 {
        let plant = p.place(ada, "final", "steamplant", 36, 8 + k * 4);
        p.wire(ada, "final", coal_f, plant, "Coal");
        p.wire(ada, "final", bay_wf, plant, "Water");
        p.wire(ada, "final", plant, bay_pf, "Power");
    }
    p.say("two plants for the floor -- flat, and nowhere near the surge");
    let pulse = p.place(ada, "final", "pulseplant", 36, 18);
    p.wire(ada, "final", coal_f, pulse, "Coal");
    p.wire(ada, "final", bay_wf, pulse, "Water");
    p.wire(ada, "final", pulse, bay_pf, "Power");
    p.say("and a pulse plant: 362 MW every seven seconds, and nothing in between");

    if !p.until("final", "Final Works", 700) {
        return finish(p);
    }

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

fn fail(e: &str) -> i32 {
    eprintln!("\x1b[31m{e}\x1b[0m");
    1
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
                design: None,
                example: true,
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
                design: None,
                example: true,
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
