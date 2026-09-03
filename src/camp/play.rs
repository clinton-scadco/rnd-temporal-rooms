//! The campaign, played end to end -- and the acceptance test of the whole
//! prototype.
//!
//! It lives in the library rather than in `bin/camp.rs` because it is two
//! things at once, and they must not be allowed to drift apart: `camp play`
//! narrates it, and `tests/camp.rs` asserts on it. A playthrough that only the
//! binary could run would be a demonstration; one only the test could run
//! would be a fixture nobody watches.
//!
//! ```text
//!   for each of five rooms:
//!     put down empty chassis
//!     design each of them, one component at a time
//!     wire the room up
//!     open the supply lines the map allows
//!     run the clock until the objective is met
//!   and compare every replica against its host, throughout
//! ```
//!
//! # It designs everything it builds
//!
//! Nothing here is placed with a finished design in its hand. A placement puts
//! down an *empty chassis* -- which is all a placement is, since experiment 13
//! -- and then the machine is drawn: a draft opened, components put down one
//! at a time, the two or three that need tuning tuned, wires made, and the
//! whole thing committed at one tick. That is [`crate::mp::cmd::draw`], and it
//! is the same stream of commands a player's hands make at the bench.
//!
//! It is worth doing the long way for one reason: it is the only way this
//! playthrough exercises the loop the game actually has. A harness that handed
//! the server finished documents proved that the *simulator* worked and said
//! nothing about whether a machine could be built at all -- which is exactly
//! how a campaign that could not be entered after somebody placed an empty
//! steam plant got as far as a play session.
//!
//! The designs it draws come from the book -- the same `.machine` documents
//! experiments 06 to 10 argued about. A harness that invented its own would be
//! testing an author rather than a game.

use super::run::Camp;
use super::site;
use crate::machine::design::Design;
use crate::model::Tick;
use crate::mp::cmd::{self, Act};
use crate::mp::goal::commas;
use crate::mp::secs;
use crate::mp::world::{head_design, stock_design, Id, PlayerId};

/// A campaign with a clock somebody else is turning, and a running commentary.
pub struct Play {
    pub c: Camp,
    /// Seconds, because everything in the script is written in them.
    pub t: u64,
    /// Everything that went wrong, in the words it went wrong in. Empty is the
    /// only passing answer.
    pub bad: Vec<String>,
    pub checks: u64,
    /// Whether the commentary is printed. `camp play` wants it; a test wants
    /// the same run without four hundred lines of it.
    pub loud: bool,
}

impl Play {
    pub fn open(seed: u64) -> Play {
        let mut c = Camp::open(seed);
        c.start_manual();
        Play { c, t: 0, bad: Vec::new(), checks: 0, loud: true }
    }

    /// The same, silent: the shape a test wants.
    pub fn quiet(seed: u64) -> Play {
        Play { loud: false, ..Play::open(seed) }
    }

    fn rule(&self, title: &str) {
        if self.loud {
            println!("\n\x1b[1m{title}\x1b[0m");
            println!("{}", "-".repeat(title.len().max(8)));
        }
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
        if self.loud {
            println!("  {:<10}{what}", format!("t+{}s", self.t));
        }
    }

    fn warn(&self, what: &str) {
        if self.loud {
            println!("  {:<10}\x1b[31m{what}\x1b[0m", format!("t+{}s", self.t));
        }
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
                self.warn(&format!("{what}  REFUSED: {e}"));
                self.bad.push(format!("t+{}s: {what}: {e}", self.t));
                0
            }
        }
    }

    /// Put something down, and -- if it is a thing with an inside -- design it.
    ///
    /// A placement is an empty chassis and nothing else. What makes it a steam
    /// plant is the stream that follows: a draft opened, six components put
    /// down, a throttle turned to 40%, eight wires made, and a commit. The
    /// design comes out of the book; the *building of it* is the game's own
    /// loop, run here rather than skipped.
    fn place(&mut self, who: PlayerId, site: &str, proto: &str, x: i32, y: i32) -> Id {
        let storage = matches!(proto, "bay" | "yard");
        let act = if storage {
            Act::PlaceStorage { proto: proto.into(), x, y, face: 0 }
        } else {
            Act::PlaceMachine { proto: proto.into(), x, y, face: 0, item: None, design: None }
        };
        let id = self.act(who, site, "", act);
        if id != 0 && !storage {
            if let Ok(d) = stock_design(proto) {
                self.design(who, site, id, proto, &d);
            }
        }
        id
    }

    /// One machine, drawn at the bench: every command a player's hands would
    /// make, in the order they would make them.
    ///
    /// The draft is compared with what it should be before it is committed.
    /// A stream that built a *nearly* identical machine would otherwise pass,
    /// and a nearly identical machine is a different factory.
    fn design(&mut self, who: PlayerId, site: &str, id: Id, what: &str, d: &Design) {
        for act in cmd::draw(id, d) {
            let verb = act.verb();
            if let Err(e) = self.c.submit(who, site, act) {
                self.warn(&format!("{what} in {site}: {verb} refused: {e}"));
                self.bad.push(format!("t+{}s: drawing {what} in {site}: {verb}: {e}", self.t));
                return;
            }
        }
        let want = cmd::redrawn(d).emit();
        let got = self
            .c
            .yard(site)
            .and_then(|y| y.room.host.world.get(id))
            .and_then(|i| i.design.as_ref().map(|d| d.emit()));
        if got.as_deref() != Some(want.as_str()) {
            self.bad.push(format!(
                "t+{}s: {what} in {site} is not what was drawn into it",
                self.t
            ));
        }
    }

    /// An extraction head on the n-th patch of ground of one kind, `dx` tiles
    /// along from its corner.
    ///
    /// Since experiment 13 a room comes with ground rather than with working
    /// mines, so the campaign builds its own -- which is the point: the first
    /// thing this playthrough does in every room is decide how to get material
    /// out of it. And a seam is a budget rather than a socket, so `dx` is how
    /// a second or third head goes down beside the first.
    ///
    /// The head is put down empty and then drawn, inlet by inlet, exactly as
    /// a player would: it is the first machine anybody designs in this game,
    /// and the campaign should not be able to finish without proving that it
    /// can be. The four designs it draws come from the book.
    fn head(&mut self, who: PlayerId, site: &str, item: &'static str, n: usize, dx: i32) -> Id {
        let at = self
            .c
            .yard(site)
            .and_then(|y| y.room.host.world.nth_ground(item, n))
            .map(|d| (d.x + dx, d.y));
        let Some((x, y)) = at else { return 0 };
        let id = self.act(
            who,
            site,
            "",
            Act::PlaceMachine { proto: "head".into(), x, y, face: 0, item: None, design: None },
        );
        if id != 0 {
            if let Ok(d) = head_design(item) {
                self.design(who, site, id, "a head", &d);
            }
        }
        id
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
                if self.loud {
                    println!(
                        "  {:<10}\x1b[32m{tag} met at {}\x1b[0m",
                        format!("t+{}s", self.t),
                        clock(at)
                    );
                }
                return true;
            }
        }
        self.warn(&format!("{tag} did not finish inside {cap}s"));
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
        if !self.loud {
            return;
        }
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

pub fn fmt(n: f64) -> String {
    if n >= 1000.0 {
        commas(n as u64)
    } else {
        format!("{n:.1}")
    }
}

pub fn clock(t: Tick) -> String {
    let s = t / 60;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The whole campaign, played: five rooms, in the order the map allows, with
/// every machine drawn out of components on the way.
///
/// Answers false only when it could not get to the end at all -- a room that
/// never finished, or a refusal there was no carrying on from. Everything else
/// it noticed is in [`Play::bad`], which is the list a caller asserts on.
pub fn run(p: &mut Play) -> bool {
    p.rule(&format!("campaign {} -- five rooms, one clock", p.c.code));
    if p.loud {
        for s in site::SITES {
            println!("  {:<10}{:<16} {}", s.tag, s.title, s.problem);
        }
    }

    let ada = match p.c.join("Ada") {
        Ok(id) => id,
        Err(e) => return stop(p, &e),
    };
    let bruno = match p.c.join("Bruno") {
        Ok(id) => id,
        Err(e) => return stop(p, &e),
    };
    if p.loud {
        println!("\n  two players, five reconstructions each, compared every simulated second.\n");
    }

    // ================================================== 1. Coal Basin
    p.rule("Coal Basin -- a platform too small for the plant it needs");
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
    let grid = fixture(p, "basin", "grid", 0);
    let coal_out = fixture(p, "basin", "depot", 0);
    let seam = p.head(ada, "basin", "Coal", 0, 0);
    let intake = p.head(ada, "basin", "Water", 0, 0);
    // The export seam is worth nine hundred a second and one head lifts four
    // hundred, so it takes three of them standing side by side -- which is the
    // trade this room is about, since every tile they cost is a tile the
    // plants wanted. The third one only gets the hundred that is left.
    let seam2 = p.head(ada, "basin", "Coal", 1, 0);
    let seam2b = p.head(ada, "basin", "Coal", 1, 2);
    let seam2c = p.head(ada, "basin", "Coal", 1, 4);

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
            design: stock_design("stamping").ok(),
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
        return false;
    }
    p.say(&format!(
        "unlocked: {}",
        p.c.tech.earned().iter().map(|u| u.title).collect::<Vec<_>>().join(", ")
    ));

    // ================================================== 2. Iron Valley
    p.rule("Iron Valley -- all the land in the world, and no fuel");
    if let Err(e) = p.c.travel(bruno, "valley") {
        return stop(p, &e);
    }
    p.say("Bruno walks to Iron Valley. Coal Basin keeps running behind him.");
    match p.c.open_route(ada, "basin", "valley", "Coal", "train", Some(200)) {
        Ok(_) => p.say("a train starts running coal, Basin to Valley: 50 seconds, 30,000 a load"),
        Err(e) => p.bad.push(format!("the coal line was refused: {e}")),
    }

    let mine1 = p.head(ada, "valley", "IronOre", 0, 0);
    let mine2 = p.head(ada, "valley", "IronOre", 1, 0);
    let seam_v = p.head(ada, "valley", "Coal", 0, 0);
    let water_v = p.head(ada, "valley", "Water", 0, 0);
    let coal_in = fixture(p, "valley", "yard", 0);
    let powder_out = fixture(p, "valley", "depot", 0);

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
        return false;
    }
    p.say(&format!(
        "unlocked: {}",
        p.c.yard("valley")
            .map(|y| y.site.unlocks().iter().map(|u| u.title).collect::<Vec<_>>().join(", "))
            .unwrap_or_default()
    ));

    // ================================================== 3. Power Station
    p.rule("Power Station -- every lump of coal a minute away");
    if let Err(e) = p.c.travel(ada, "station") {
        return stop(p, &e);
    }
    match p.c.open_route(ada, "basin", "station", "Coal", "train", Some(140)) {
        Ok(_) => p.say("a second train: Basin to the Station, 30,000 a load"),
        Err(e) => p.bad.push(format!("the station's coal line was refused: {e}")),
    }
    let water_s = p.head(ada, "station", "Water", 0, 0);
    let coal_s = fixture(p, "station", "yard", 0);
    let grid_s = fixture(p, "station", "grid", 0);
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
        return false;
    }

    // ================================================== 4. Manufacturing
    p.rule("Manufacturing -- no coal, no water, no grid");
    if let Err(e) = p.c.travel(bruno, "works") {
        return stop(p, &e);
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
    let caster = p.head(ada, "works", "IronBillet", 0, 0);
    let coal_w = fixture(p, "works", "yard", 0);
    let pow_w = fixture(p, "works", "yard", 1);
    let gear_out = fixture(p, "works", "depot", 0);
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
        return false;
    }

    // ============================ 5. back to Iron Valley, then Final Works
    p.rule("Iron Valley, again -- the separator changes what that room is for");
    if let Err(e) = p.c.travel(bruno, "valley") {
        return stop(p, &e);
    }
    p.say("Bruno goes back. The powder line has been running the whole time.");
    p.at(p.t + 4);
    let conc_out = fixture(p, "valley", "depot", 1);
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

    p.rule("Final Works -- a load that will not sit still");
    if let Err(e) = p.c.travel(ada, "final") {
        return stop(p, &e);
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
    let water_f = p.head(ada, "final", "Water", 0, 0);
    let coal_f = fixture(p, "final", "yard", 0);
    let gear_in = fixture(p, "final", "yard", 1);
    let conc_in = fixture(p, "final", "yard", 2);
    let grid_f = fixture(p, "final", "grid", 0);
    let gear_ship = fixture(p, "final", "depot", 0);
    let conc_ship = fixture(p, "final", "depot", 1);
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
        return false;
    }

    true
}

/// The playthrough could not carry on. Said once, kept, and answered `false`.
fn stop(p: &mut Play, why: &str) -> bool {
    p.warn(why);
    p.bad.push(why.to_string());
    false
}

