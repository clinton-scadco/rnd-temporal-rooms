//! The slice, played end to end -- and the acceptance test of the whole
//! experiment.
//!
//! It lives in the library rather than in `bin/slice.rs` because it is two
//! things at once and they must not be allowed to drift apart: `slice play`
//! narrates it and `tests/slice.rs` asserts on it.
//!
//! The brief's success criterion is a sentence about what players do:
//!
//! ```text
//!   mine ore cheaply in 1890  ->  move it through temporal logistics
//!                             ->  process with better 2037 machinery
//! ```
//!
//! so this script does exactly that, in that order, with every machine drawn
//! one component at a time out of the book -- and then does the harder half,
//! which is carrying a generator *backwards* and standing 1890's own power
//! station up out of it.
//!
//! ```text
//!   1. the valley mines, and mills powder on water          (nothing imported)
//!   2. the district bootstraps a grid off its last coal     (nothing imported)
//!   3. the district opens the deep fracture                 (98 MW, held)
//!   4. carts bring 1890 ore and 1890 coal forward           (147 years)
//!   5. the district crushes it into 2037 concentrate        (the criterion)
//!   6. the district presses gears and sends them back       (machinery)
//!   7. the valley builds a hydro station out of two crates  (the other half)
//!   8. the zone turns 2037 billet into gears on lathes      (the far end)
//! ```
//!
//! # What it proves by being refused
//!
//! Three of the steps are deliberately attempted too early, and the refusal is
//! the point rather than a hazard:
//!
//! ```text
//!   a bay on Kestrel Reach in 2037     somebody built there in 1951
//!   a grid connection in 1890          a wire with nothing on the end of it
//!   a hydro station before the gears   3,200 short of the machinery
//! ```
//!
//! The first two are walls and say so. The third is a *price*, and the
//! playthrough goes on to pay it.

use super::gate;
use super::phase::Phase;
use super::run::Slice;
use crate::machine::design::Design;
use crate::model::Tick;
use crate::mp::cmd::{self, Act};
use crate::mp::goal::commas;
use crate::mp::secs;
use crate::mp::world::{head_design, stock_design, Id, PlayerId};

/// The two designs this experiment added to the book.
pub const WATERMILL: &str = include_str!("../../designs/23-watermill.machine");
pub const HYDRO: &str = include_str!("../../designs/24-hydro.machine");

/// A slice with a clock somebody else is turning, and a running commentary.
pub struct Play {
    pub s: Slice,
    /// Seconds, because everything in the script is written in them.
    pub t: u64,
    /// Everything that went wrong, in the words it went wrong in. Empty is the
    /// only passing answer.
    pub bad: Vec<String>,
    pub checks: u64,
    pub loud: bool,
}

impl Play {
    pub fn open(seed: u64) -> Play {
        let mut s = Slice::open(seed);
        s.start_manual();
        Play { s, t: 0, bad: Vec::new(), checks: 0, loud: true }
    }

    pub fn quiet(seed: u64) -> Play {
        Play { loud: false, ..Play::open(seed) }
    }

    fn rule(&self, title: &str) {
        if self.loud {
            println!("\n\x1b[1m{title}\x1b[0m");
            println!("{}", "-".repeat(title.len().max(8)));
        }
    }

    /// Move the clock to `s` seconds, never backwards, and let the slice catch
    /// up -- which advances all three regions, lands whatever was in the air,
    /// asks every interface whether it is holding, and loads whatever the
    /// depots have shipped.
    fn at(&mut self, s: u64) {
        self.t = self.t.max(s);
        self.s.set_now(secs(self.t));
        if let Err(e) = self.s.advance() {
            self.bad.push(format!("t+{}s: the slice would not run: {e}", self.t));
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

    fn good(&self, what: &str) {
        if self.loud {
            println!("  {:<10}\x1b[32m{what}\x1b[0m", format!("t+{}s", self.t));
        }
    }

    /// One intention, and the id of whatever it put down.
    fn act(&mut self, who: PlayerId, region: &str, what: &str, act: Act) -> Id {
        match self.s.submit(who, region, act) {
            Ok(_) => {
                let id = self
                    .s
                    .yard(region)
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

    /// Something that is *supposed* to be refused, and the sentence it is
    /// refused with. A refusal that did not happen is the failure here.
    fn expect_refusal(&mut self, who: PlayerId, region: &str, what: &str, act: Act) {
        match self.s.submit(who, region, act) {
            Ok(_) => {
                self.warn(&format!("{what}  was ALLOWED, and should not have been"));
                self.bad.push(format!("t+{}s: {what} was allowed", self.t));
            }
            Err(e) => self.good(&format!("{what} is refused: {e}")),
        }
    }

    /// Put an empty chassis down and, if it has an inside, draw a design into
    /// it one component at a time.
    fn place(&mut self, who: PlayerId, region: &str, proto: &str, x: i32, y: i32) -> Id {
        let storage = matches!(proto, "bay" | "yard");
        let act = if storage {
            Act::PlaceStorage { proto: proto.into(), x, y, face: 0 }
        } else {
            Act::PlaceMachine { proto: proto.into(), x, y, face: 0, item: None, design: None }
        };
        let id = self.act(who, region, "", act);
        if id != 0 && !storage {
            if let Ok(d) = stock_design(proto) {
                self.design(who, region, id, proto, &d);
            }
        }
        id
    }

    /// The same, with a design of this experiment's own rather than the
    /// prototype's stock one.
    ///
    /// This is the join between the two halves of the repository that matters
    /// most here: a chassis is era-neutral, and what makes the thing standing on
    /// it a 1890 machine is the document drawn into it. There is no Water Powder
    /// Line prototype and there should not be -- there is a powder line, and it
    /// has water wheels in it because that is what somebody drew.
    fn place_with(
        &mut self,
        who: PlayerId,
        region: &str,
        proto: &str,
        x: i32,
        y: i32,
        src: &str,
        what: &str,
    ) -> Id {
        let Ok(d) = Design::parse(src) else {
            self.bad.push(format!("{what} will not parse"));
            return 0;
        };
        let id = self.act(
            who,
            region,
            "",
            Act::PlaceMachine { proto: proto.into(), x, y, face: 0, item: None, design: None },
        );
        if id == 0 {
            // `act` has already said what went wrong; this says what it was for.
            self.bad.push(format!("t+{}s: there was no room for {what}", self.t));
            return 0;
        }
        self.design(who, region, id, what, &d);
        id
    }

    /// A depot of one item, placed by a player rather than furnished.
    ///
    /// The valley uses one as a tailrace: a water wheel gives its water back,
    /// and in world terms that is an output which has to go somewhere. A depot
    /// set to water *is* a tailrace, and it costs the region nothing, because
    /// nothing anybody was asked for is measured in water.
    fn sink(&mut self, who: PlayerId, region: &str, item: &str, x: i32, y: i32) -> Id {
        self.act(
            who,
            region,
            "",
            Act::PlaceMachine {
                proto: "depot".into(),
                x,
                y,
                face: 0,
                item: Some(item.to_string()),
                design: None,
            },
        )
    }

    /// One machine, drawn at the bench: every command a player's hands would
    /// make, in the order they would make them.
    fn design(&mut self, who: PlayerId, region: &str, id: Id, what: &str, d: &Design) {
        for act in cmd::draw(id, d) {
            let verb = act.verb();
            if let Err(e) = self.s.submit(who, region, act) {
                self.warn(&format!("{what} in {region}: {verb} refused: {e}"));
                self.bad.push(format!("t+{}s: drawing {what} in {region}: {verb}: {e}", self.t));
                return;
            }
        }
        let want = cmd::redrawn(d).emit();
        let got = self
            .s
            .yard(region)
            .and_then(|y| y.room.host.world.get(id))
            .and_then(|i| i.design.as_ref().map(|d| d.emit()));
        if got.as_deref() != Some(want.as_str()) {
            self.bad.push(format!("t+{}s: {what} in {region} is not what was drawn into it", self.t));
        }
    }

    /// An extraction head on the n-th patch of ground of one kind, `dx` tiles
    /// along from its corner.
    fn head(&mut self, who: PlayerId, region: &str, item: &'static str, n: usize, dx: i32) -> Id {
        let at = self
            .s
            .yard(region)
            .and_then(|y| y.room.host.world.nth_ground(item, n))
            .map(|d| (d.x + dx, d.y));
        let Some((x, y)) = at else {
            self.bad.push(format!("{region} has no {item} ground at index {n}"));
            return 0;
        };
        let id = self.act(
            who,
            region,
            "",
            Act::PlaceMachine { proto: "head".into(), x, y, face: 0, item: None, design: None },
        );
        if id != 0 {
            if let Ok(d) = head_design(item) {
                self.design(who, region, id, "a head", &d);
            }
        }
        id
    }

    fn wire(&mut self, who: PlayerId, region: &str, from: Id, to: Id, item: &str) {
        self.act(who, region, "", Act::CreateConnection { from, to, item: item.into() });
    }

    /// Run until a region's objective is met, or give up and say so.
    fn until(&mut self, region: &str, tag: &str, cap: u64) -> bool {
        let start = self.t;
        while self.t < start + cap {
            self.tick(5);
            self.probe();
            if self.s.yard(region).and_then(|y| y.done_at()).is_some() {
                let at = self.s.yard(region).and_then(|y| y.done_at()).unwrap_or(0);
                self.good(&format!("{tag} met at {}", clock(at)));
                return true;
            }
        }
        self.warn(&format!("{tag} did not finish inside {cap}s"));
        self.report(region);
        self.bad.push(format!("{tag} never met its objective"));
        false
    }

    /// Run for a while without waiting for anything, keeping the replicas
    /// current as we go.
    fn run_for(&mut self, ds: u64) {
        let want = self.t + ds;
        while self.t < want {
            self.tick(5);
            self.probe();
        }
    }

    /// Every replica of every region, against its host.
    fn probe(&mut self) {
        for id in self.s.cast.iter().map(|c| c.id).collect::<Vec<_>>() {
            if let Err(e) = self.s.sync_all(id) {
                self.bad.push(format!("t+{}s: player {id} could not be synchronised: {e}", self.t));
            }
        }
        self.checks += 1;
        if !self.s.agrees() {
            self.bad.push(format!("t+{}s: a replica disagreed with its host", self.t));
        }
    }

    /// Why a region is not finished, in the region's own words.
    fn report(&mut self, region: &str) {
        if !self.loud {
            return;
        }
        let Some(y) = self.s.yard(region) else { return };
        for l in y.room.host.progress().lines {
            println!(
                "               {:<40} {:>12} / {} {}",
                l.what, fmt(l.have), fmt(l.need), l.unit
            );
        }
        for (id, why) in &y.room.host.build.idle {
            let name = y.room.host.world.get(*id).map(|i| i.name.clone()).unwrap_or_default();
            println!("               \x1b[33m{name} is idle: {why}\x1b[0m");
        }
    }

    /// One fixture of a region, by prototype and order.
    fn fixture(&self, region: &str, tag: &str, n: usize) -> Id {
        self.s
            .yard(region)
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
    }

    /// The yard a named item lands in.
    fn takes(&self, region: &str, item: &str) -> Id {
        self.s
            .yard(region)
            .and_then(|y| y.ports.incoming.get(item).copied())
            .unwrap_or(0)
    }

    /// The depot a named item leaves from.
    fn gives(&self, region: &str, item: &str) -> Id {
        self.s
            .yard(region)
            .and_then(|y| y.ports.outgoing.get(item).copied())
            .unwrap_or(0)
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

// ==========================================================================

/// The whole slice, played: three centuries of one valley, in the order the
/// fractures allow.
///
/// Answers false only when it could not get to the end at all. Everything else
pub fn run(p: &mut Play) -> bool {
    p.rule(&format!("slice {} -- one valley, three centuries", p.s.code));
    if p.loud {
        for r in super::region::REGIONS {
            println!("  {:<10}{:<28} {}", r.phase.tag(), r.title, r.problem);
        }
        println!();
        for f in gate::FRACTURES {
            println!(
                "  {:<10}{} -> {}, {} years, {} MW off {}'s grid{}",
                f.tag,
                f.early,
                f.late,
                f.gap(),
                gate::draw(f.gap()),
                f.held_by,
                if f.standing { ", already standing" } else { "" }
            );
        }
    }

    let ada = match p.s.join("Ada") {
        Ok(id) => id,
        Err(e) => return stop(p, &e),
    };
    let bruno = match p.s.join("Bruno") {
        Ok(id) => id,
        Err(e) => return stop(p, &e),
    };

    // ==================================================== 1. the valley, 1890
    p.rule("1890 Mining Valley -- ore a shovel deep, and carts to move it with");
    p.at(4);

    let ore_out = p.gives("valley", "IronOre");
    let coal_out = p.gives("valley", "Coal");
    let powder_out = p.gives("valley", "OrePowder");

    // Three ore bodies. The richest of them is the one that will have somebody
    // else's foundry standing on it in a hundred and sixty years.
    let kestrel = p.head(ada, "valley", "IronOre", 0, 0);
    let spoil = p.head(ada, "valley", "IronOre", 1, 0);
    let lowfield = p.head(ada, "valley", "IronOre", 2, 0);
    // Blackband is eight hundred a second and a head lifts four, so two of them.
    let coal_v = p.head(ada, "valley", "Coal", 0, 0);
    let coal_v2 = p.head(ada, "valley", "Coal", 0, 4);
    let water_v1 = p.head(ada, "valley", "Water", 0, 0);
    let water_v2 = p.head(ada, "valley", "Water", 0, 8);
    p.say("seven heads: three ore bodies, two on Blackband, and two on the mill race");

    // Two ore bays, and which head goes into which is the first real decision
    // in this region.
    //
    // A delivery depot swallows twelve hundred a second and a bay arbitrates
    // round-robin, so a depot and a machine sharing one bay is a depot with a
    // machine starving behind it. Kestrel's four hundred goes to the export
    // yard; the Spoil and Lowfield heads feed the mill, which wants ninety.
    let yard_ore = p.place(ada, "valley", "yard", 0, 58);
    let bay_mill = p.place(ada, "valley", "bay", 10, 58);
    let bay_coal_v = p.place(ada, "valley", "bay", 16, 58);
    let yard_water_v = p.place(ada, "valley", "yard", 22, 58);
    let bay_powder = p.place(ada, "valley", "bay", 32, 58);
    p.wire(ada, "valley", kestrel, yard_ore, "IronOre");
    p.wire(ada, "valley", spoil, bay_mill, "IronOre");
    p.wire(ada, "valley", lowfield, bay_mill, "IronOre");
    p.wire(ada, "valley", coal_v, bay_coal_v, "Coal");
    p.wire(ada, "valley", coal_v2, bay_coal_v, "Coal");
    p.wire(ada, "valley", water_v1, yard_water_v, "Water");
    p.wire(ada, "valley", water_v2, yard_water_v, "Water");
    p.wire(ada, "valley", yard_ore, ore_out, "IronOre");
    p.wire(ada, "valley", bay_coal_v, coal_out, "Coal");
    p.wire(ada, "valley", bay_powder, powder_out, "OrePowder");
    p.say("400 ore a second into the export yard, 400 more standing by for the mill");

    // The one machine this century can build that does anything but lift.
    p.at(10);
    let mill = p.place_with(
        ada,
        "valley",
        "powderline",
        38,
        58,
        WATERMILL,
        "a water powder line: four wheels, a pulley geared up four times, and a mill",
    );
    let tail = p.sink(ada, "valley", "Water", 46, 58);
    p.wire(ada, "valley", bay_mill, mill, "IronOre");
    p.wire(ada, "valley", yard_water_v, mill, "Water");
    p.wire(ada, "valley", mill, bay_powder, "OrePowder");
    p.wire(ada, "valley", mill, tail, "Water");
    p.say("and a tailrace, because a wheel gives its water back");

    // The wall that really is a wall: there is no grid, and no crate of wire
    // fixes that.
    p.expect_refusal(
        ada,
        "valley",
        "a stock powder line off a grid that does not exist",
        Act::PlaceMachine {
            proto: "powderline".into(),
            x: 56,
            y: 65,
            face: 0,
            item: None,
            design: stock_design("powderline").ok(),
        },
    );

    // ================================================= 2. the district, 2037
    p.rule("2037 Industrial District -- every machine you want, and nothing to put in one");
    if let Err(e) = p.s.travel(bruno, "district") {
        return stop(p, &e);
    }
    p.at(p.t + 4);

    // The wall that is the whole experiment: the same tiles, a hundred and
    // forty-seven years later, with somebody else's foundry on them.
    let kestrel_at = super::land::feature("kestrel").map(|f| (f.x, f.y)).unwrap_or((8, 6));
    p.expect_refusal(
        bruno,
        "district",
        "a bay on Kestrel Reach",
        Act::PlaceStorage { proto: "bay".into(), x: kestrel_at.0, y: kestrel_at.1, face: 0 },
    );

    let grid_d = p.fixture("district", "grid", 0);
    let conc_out = p.gives("district", "Concentrate");
    let billet_out = p.gives("district", "IronBillet");
    let gear_out_d = p.gives("district", "Gear");
    let ore_in = p.takes("district", "IronOre");
    let coal_in = p.takes("district", "Coal");
    let gear_in_d = p.takes("district", "Gear");

    // Two intakes, not three: a pump head lifts eight hundred and the culverted
    // race is twelve hundred, so a third would stand on ground already spoken
    // for and turn at nothing -- which the compiler refuses by name.
    let water_d1 = p.head(bruno, "district", "Water", 0, 0);
    let water_d2 = p.head(bruno, "district", "Water", 0, 8);
    let coal_d = p.head(bruno, "district", "Coal", 0, 0);
    // Two casters on Tarrant for the same reason the valley has two ore bays.
    let billet_d = p.head(bruno, "district", "IronBillet", 0, 0);
    let billet_d2 = p.head(bruno, "district", "IronBillet", 0, 4);
    p.say("two intakes on the culverted race, the last of Blackband, and two casters");

    let yard_water_d = p.place(bruno, "district", "yard", 0, 58);
    let bay_coal_d = p.place(bruno, "district", "bay", 10, 58);
    let yard_grid = p.place(bruno, "district", "yard", 16, 58);
    let bay_billet = p.place(bruno, "district", "bay", 26, 58);
    let bay_export = p.place(bruno, "district", "bay", 32, 58);
    let bay_conc = p.place(bruno, "district", "bay", 38, 58);
    p.wire(bruno, "district", water_d1, yard_water_d, "Water");
    p.wire(bruno, "district", water_d2, yard_water_d, "Water");
    p.wire(bruno, "district", coal_d, bay_coal_d, "Coal");
    p.wire(bruno, "district", billet_d, bay_billet, "IronBillet");
    p.wire(bruno, "district", billet_d2, bay_export, "IronBillet");
    p.wire(bruno, "district", yard_grid, grid_d, "Power");
    p.wire(bruno, "district", bay_conc, conc_out, "Concentrate");
    p.wire(bruno, "district", bay_export, billet_out, "IronBillet");
    // The district is a *staging post* for machinery as well as a factory: what
    // lands in its gear yard out of 2070 goes straight back out of its gear
    // dock, bound for 1890, and is still 2070 machinery when it gets there.
    p.wire(bruno, "district", gear_in_d, gear_out_d, "Gear");

    // The bootstrap, and it is the nicest accident in the experiment. The two
    // fractures want 151 MW between them; forty-five a second of coal will keep
    // one compact plant alight, which is 108; and the rest of it comes off the
    // river, on a generator 2037 can build for itself.
    p.at(p.t + 6);
    let plant_a = p.place(bruno, "district", "steamplant", 0, 65);
    p.wire(bruno, "district", bay_coal_d, plant_a, "Coal");
    p.wire(bruno, "district", yard_water_d, plant_a, "Water");
    p.wire(bruno, "district", plant_a, yard_grid, "Power");
    p.say("one compact plant on the last of the local coal: 108 MW");

    let hydro_d = p.place_with(
        bruno,
        "district",
        "turbinehall",
        5,
        65,
        HYDRO,
        "a hydro station on the old mill race: 126 MW and no fuel at all",
    );
    let tail_d = p.sink(bruno, "district", "Water", 10, 65);
    p.wire(bruno, "district", yard_water_d, hydro_d, "Water");
    p.wire(bruno, "district", hydro_d, yard_grid, "Power");
    p.wire(bruno, "district", hydro_d, tail_d, "Water");
    p.say("2037 opens a hundred-and-forty-seven-year fracture on water power");

    p.run_for(20);
    let mw = p.s.yard("district").map(|y| y.grid_mw()).unwrap_or(0);
    p.say(&format!("the district's grid connection is taking {mw} MW"));

    // ============================================ 3. the deep fracture opens
    p.rule("The deep fracture -- 147 years, 98 MW, and a gantry");
    match p.s.open_gate(bruno, "deep") {
        Ok(_) => p.say("an interface goes up on the rift"),
        Err(e) => return stop(p, &e),
    }
    p.run_for(15);
    match p.s.ledger.gate(0) {
        Some(g) if g.lit => p.good(&format!(
            "the deep fracture is holding {} years open on {} MW",
            g.worst, g.want_mw
        )),
        Some(g) => {
            p.warn(&g.why_dark().unwrap_or_default());
            p.bad.push("the deep fracture would not light".into());
        }
        None => p.bad.push("there is no interface on the deep fracture".into()),
    }

    // ================================= 4. carts, forward, a hundred and fifty
    p.rule("Temporal logistics -- carts and a tramway, 1890 to 2037");
    for (item, fleet, cap) in [
        ("IronOre", "tramway", 120u64),
        ("Coal", "tramway", 380),
        ("Coal", "wagon", 140),
        ("OrePowder", "wagon", 40),
    ] {
        match p.s.open_route(ada, "valley", "district", item, fleet, Some(cap)) {
            Ok(_) => p.say(&format!("{item} runs valley -> district by {fleet}, {cap} a second")),
            Err(e) => p.bad.push(format!("the {item} run was refused: {e}")),
        }
    }
    // A train cannot be fielded in 1890, and the refusal says why rather than
    // saying no.
    match p.s.open_route(ada, "valley", "district", "IronOre", "train", None) {
        Ok(_) => p.bad.push("a train ran out of 1890".into()),
        Err(e) => p.good(&format!("a train out of 1890 is refused: {e}")),
    }

    // ================================= 5. and the district crushes 1890's ore
    p.rule("The criterion -- ore mined in 1890, crushed by machinery from 2037");
    p.run_for(60);
    p.say(&format!(
        "{} ore and {} coal have come forward through the fracture",
        commas(landed_from(p, "district", "IronOre", Phase::P1890)),
        commas(landed_from(p, "district", "Coal", Phase::P1890))
    ));

    // The hydro station was the bootstrap and now it is in the way. On this
    // river a megawatt of steam costs a quarter of the water a megawatt of
    // falling water does, and the crusher wants three hundred and twenty-six a
    // second of it -- so the moment 1890's coal is arriving, the wheels come
    // down. Which is a decision the valley itself never gets to make, because
    // the valley has no boiler to make it with.
    p.at(p.t + 4);
    if let Err(e) = p.s.submit(bruno, "district", Act::DeleteMachine { id: hydro_d }) {
        p.bad.push(format!("the hydro station could not be taken down: {e}"));
    } else {
        p.say("the wheels come down: 766 water a second back, and 126 MW gone");
    }
    for k in 0..4 {
        let plant = p.place(bruno, "district", "steamplant", 16 + k * 5, 65);
        p.wire(bruno, "district", coal_in, plant, "Coal");
        p.wire(bruno, "district", yard_water_d, plant, "Water");
        p.wire(bruno, "district", plant, yard_grid, "Power");
    }
    p.say("four compact plants in their place, burning coal out of 1890: 540 MW on the grid");

    p.at(p.t + 4);
    let crusher = p.place(bruno, "district", "crusher", 36, 65);
    p.wire(bruno, "district", ore_in, crusher, "IronOre");
    p.wire(bruno, "district", coal_in, crusher, "Coal");
    p.wire(bruno, "district", yard_water_d, crusher, "Water");
    p.wire(bruno, "district", crusher, bay_conc, "Concentrate");
    p.say("a steam crusher: 93 ore a second in, 38 concentrate out, on 100 coal of its own");

    // The valley's own objective is met while all of that is going on, and the
    // ore it is shipping is the same ore.
    if !p.until("valley", "1890 Mining Valley", 500) {
        return false;
    }

    // ======================================= 6. the far end: 2070's lathes
    p.rule("2070 Manufacturing Zone -- one machine where three were");
    if let Err(e) = p.s.travel(ada, "zone") {
        return stop(p, &e);
    }
    for (item, fleet, cap) in [("IronBillet", "train", 120u64), ("Power", "train", 200)] {
        match p.s.open_route(ada, "district", "zone", item, fleet, Some(cap)) {
            Ok(_) => p.say(&format!("{item} runs district -> zone by {fleet}, {cap} a second")),
            Err(e) => p.bad.push(format!("the {item} corridor run was refused: {e}")),
        }
    }
    p.at(p.t + 4);
    let gear_out_z = p.gives("zone", "Gear");
    // The landing yards are wired to directly rather than relayed into bays of
    // their own: two bays are joined by a transport in this game and not by a
    // wire, and a local head putting its output into the same yard an import
    // lands in is both legal and exactly right. A cell should not have to care
    // which century its billet is from.
    let billet_in = p.takes("zone", "IronBillet");
    let power_in = p.takes("zone", "Power");
    let water_z = p.head(ada, "zone", "Water", 0, 0);
    let billet_z = p.head(ada, "zone", "IronBillet", 0, 0);
    let yard_water_z = p.place(ada, "zone", "yard", 0, 58);
    let bay_gear_z = p.place(ada, "zone", "bay", 10, 58);
    p.wire(ada, "zone", water_z, yard_water_z, "Water");
    p.wire(ada, "zone", billet_z, billet_in, "IronBillet");
    p.wire(ada, "zone", bay_gear_z, gear_out_z, "Gear");
    p.say("a caster head on what is left of Tarrant, into the same yard the trains land in");

    // 2070 builds a generator without importing anything, so the old mill race
    // is worth something to it for nothing at all -- the same design, and the
    // same river, that the valley has to buy two crates for. There is no longer
    // enough water in it for both wheels, which is its own small story.
    let hydro_z = p.place_with(
        ada,
        "zone",
        "turbinehall",
        16,
        58,
        HYDRO,
        "a hydro station on the same river, 180 years on, for nothing at all",
    );
    let tail_z = p.sink(ada, "zone", "Water", 22, 58);
    p.wire(ada, "zone", yard_water_z, hydro_z, "Water");
    p.wire(ada, "zone", hydro_z, power_in, "Power");
    p.wire(ada, "zone", hydro_z, tail_z, "Water");

    for k in 0..2 {
        let cell = p.place(ada, "zone", "machining", k * 6, 65);
        p.wire(ada, "zone", billet_in, cell, "IronBillet");
        p.wire(ada, "zone", power_in, cell, "Power");
        p.wire(ada, "zone", cell, bay_gear_z, "Gear");
    }
    p.say("two machining cells: 24 gears a second each, on lathes nobody else has");

    // ============================ 7. everything runs, on one clock
    p.rule("Running -- three centuries of one valley, on one clock");
    if !p.until("district", "2037 Industrial District", 900) {
        return false;
    }
    if !p.until("zone", "2070 Manufacturing Zone", 900) {
        return false;
    }

    // ============================= 8. machinery, backwards, both fractures
    p.rule("Machinery -- gears going the wrong way down the years, twice");
    for (from, to, cap) in [("zone", "district", 80u64), ("district", "valley", 200)] {
        let fleet = if from == "zone" { "hauler" } else { "train" };
        match p.s.open_route(ada, from, to, "Gear", fleet, Some(cap)) {
            Ok(_) => p.say(&format!("gears run {from} -> {to} by {fleet}, {cap} a second")),
            Err(e) => p.bad.push(format!("the {from} -> {to} machinery run was refused: {e}")),
        }
    }
    p.say("nothing in 1890 or 2037 has a lathe, so the crates are 2070's -- which means");
    p.say("the deep fracture is about to be asked to hold a hundred and eighty years");

    let hydro_cost = super::phase::design_cost(
        &Design::parse(HYDRO).unwrap_or_else(|_| Design::empty()),
        Phase::P1890,
    );
    p.say(&format!(
        "a hydro station in 1890 needs {} gears of imported machinery",
        commas(hydro_cost)
    ));
    p.expect_refusal(
        ada,
        "valley",
        "a hydro station before the crates arrive",
        Act::PlaceMachine {
            proto: "turbinehall".into(),
            x: 54,
            y: 58,
            face: 0,
            item: None,
            design: Design::parse(HYDRO).ok(),
        },
    );

    // ============================= 9. and the valley gets its power station
    p.rule("1890, again -- electricity, out of two crates and a river");
    if let Err(e) = p.s.travel(ada, "valley") {
        return stop(p, &e);
    }
    let mut waited = 0;
    while p.s.free_machinery("valley") < hydro_cost as i64 && waited < 700 {
        p.run_for(20);
        waited += 20;
    }
    if let Some(g) = p.s.ledger.gate(0) {
        p.say(&format!(
            "the deep fracture is holding {} years open now, on {} MW instead of 98",
            g.worst, g.want_mw
        ));
        if g.worst <= gate::FRACTURES[0].gap() {
            p.bad.push(
                "2070 machinery crossed the deep fracture without widening what it holds".into(),
            );
        }
    }
    let (landed, _, free) = p.s.machinery("valley");
    p.say(&format!(
        "{} gears have come back through two fractures; {} are free",
        commas(landed),
        commas(free.max(0) as u64)
    ));
    let hydro_v = p.place_with(
        ada,
        "valley",
        "turbinehall",
        54,
        58,
        HYDRO,
        "a hydro station in 1890: four local wheels, one timber pulley, two imported generators",
    );
    let built = p
        .s
        .yard("valley")
        .and_then(|y| y.room.host.world.get(hydro_v))
        .and_then(|i| i.design.as_ref())
        .is_some();
    if !built {
        p.bad.push("the valley never got its power station".into());
    } else {
        let yard_pow_v = p.place(ada, "valley", "yard", 60, 58);
        let tail_v = p.sink(ada, "valley", "Water", 60, 66);
        p.wire(ada, "valley", yard_water_v, hydro_v, "Water");
        p.wire(ada, "valley", hydro_v, yard_pow_v, "Power");
        p.wire(ada, "valley", hydro_v, tail_v, "Water");
        p.run_for(20);
        let spent = p.s.machinery("valley").1;
        p.good(&format!(
            "1890 is generating electricity: {} gears of imported plant standing in it, and \
             every wheel turning it is local",
            commas(spent)
        ));
    }

    // ----------------------------------------------------- what it was made of
    p.rule("Provenance -- what everything is made of, and which century made it");
    if p.loud {
        for (tag, item) in [
            ("district", "IronOre"),
            ("district", "Coal"),
            ("district", "OrePowder"),
            ("district", "Gear"),
            ("valley", "Gear"),
            ("zone", "IronBillet"),
            ("zone", "Power"),
        ] {
            if let Some(b) = p.s.ledger.bin(tag, item) {
                let from: Vec<String> = b
                    .landed
                    .iter()
                    .map(|(ph, n)| format!("{} out of {}", commas(*n), ph.tag()))
                    .collect();
                if !from.is_empty() {
                    println!("  {:<10}{:<14}{}", tag, item, from.join(", "));
                }
            }
        }
    }

    // The criterion, checked rather than narrated.
    if landed_from(p, "district", "IronOre", Phase::P1890) == 0 {
        p.bad.push("no 1890 ore ever reached 2037".into());
    }
    if p.s.ledger.from_later("valley", "Gear", Phase::P1890) == 0 {
        p.bad.push("no machinery ever came back to 1890".into());
    }
    // And the thing the FIFO queue exists to make true: gears made in 2070 and
    // carried *through* 2037 are still 2070 machinery when they reach 1890.
    // Nothing anywhere says so; it is true because carrying is not making.
    if landed_from(p, "valley", "Gear", Phase::P2070) == 0 {
        p.bad.push("the gears that reached 1890 were not stamped 2070".into());
    }
    if landed_from(p, "district", "IronOre", Phase::P2037) != 0 {
        p.bad.push("1890 ore arrived in 2037 stamped as something later".into());
    }

    true
}

/// How much of one item a region has been sent out of one phase, ever.
fn landed_from(p: &Play, tag: &str, item: &str, from: Phase) -> u64 {
    p.s.ledger.bin(tag, item).and_then(|b| b.landed.get(&from).copied()).unwrap_or(0)
}

fn stop(p: &mut Play, why: &str) -> bool {
    p.warn(why);
    p.bad.push(why.to_string());
    false
}
