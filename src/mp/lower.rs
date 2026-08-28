//! The compiler between the two experiments: a machine design becomes a world
//! recipe.
//!
//! Experiment 06 refused to let a finished machine collapse into `input x
//! efficiency = output`, and compiled it to a startup transient followed by an
//! exact periodic orbit instead. That refusal is what makes this module short:
//! a machine that repeats itself every `period` ticks, having taken this and
//! given that, *is* a recipe already.
//!
//! ```text
//!   orbit:  took { Water 300, Coal 40 }  gave { Iron(gear) 940 }  over 47 ticks
//!   world:  process { consumes 300 Water 40 Coal  takes 47 s  produces 940 Gear }
//! ```
//!
//! Two conversions happen on the way across, and both are gameplay decisions
//! rather than arithmetic:
//!
//! **A designer tick is a game second.** Experiment 06's clock was its own,
//! and its numbers were chosen to read well against it -- 104 MW a tick, 20
//! gears a tick. Read at sixty ticks a second those are 6,240 MW and 1,200
//! gears a second, which is section 20 of the brief's warning about mistaking
//! resolution for pace, arriving exactly on schedule. So one orbit tick is one
//! second of world time, and the machine that made 20 gears a tick makes 20
//! gears a second.
//!
//! **A stuff becomes an item.** The outer game deals in item names and the
//! inner one in a substance with five properties, so the crossing is a
//! deliberate *grading*: ore at 84% and powder is `Concentrate`, ore at 40% is
//! `IronOre`, and iron in the shape of a gear is a `Gear`. Everything the
//! grading throws away is still visible inside the machine, which is where it
//! is a decision somebody made rather than a label.
//!
//! What does *not* cross is the transient. A committed machine begins from a
//! deterministic cold start in the world's own terms -- one class, idle,
//! asking for work -- because section 14 of the brief asks for the
//! conservative rule, and because a transient poured into a population is a
//! state nobody can name.

use super::{DESIGN_TICK, TILE_IN_DESIGN_TILES};
use crate::json::Json;
use crate::machine::design::Design;
use crate::machine::eval::{self, Report};
use crate::machine::orbit;
use crate::machine::stuff::{
    Stuff, Subst, FORM_BILLET, FORM_GEAR, FORM_SCRAP, FORM_STRIP, SIZE_POWDER,
};
use crate::model::{Qty, Tick};

/// The world item one boundary stuff counts as.
///
/// Deliberately coarse, and deliberately not automatic: the grades are the
/// game's vocabulary, and a machine whose output falls a grade short is making
/// a different item rather than a slightly worse one. That is most of the
/// reason a crushing line is worth designing at all.
pub fn item_of(s: &Stuff) -> &'static str {
    match s.subst {
        Subst::Power => "Power",
        Subst::Heat => "Heat",
        Subst::Torque => "Torque",
        Subst::Stroke => "Stroke",
        Subst::Water => "Water",
        Subst::Coal => "Coal",
        Subst::Crude => "Crude",
        Subst::Slag => "Slag",
        Subst::Light => "LightFraction",
        Subst::Middle => "MiddleFraction",
        Subst::Heavy => "HeavyFraction",
        Subst::Ore => {
            if s.q.purity >= 80 && s.q.size >= SIZE_POWDER {
                "Concentrate"
            } else if s.q.size >= SIZE_POWDER {
                "OrePowder"
            } else {
                "IronOre"
            }
        }
        Subst::Iron => match s.q.form {
            FORM_GEAR => "Gear",
            FORM_STRIP => "IronStrip",
            FORM_BILLET => "IronBillet",
            FORM_SCRAP => "Scrap",
            _ => "Iron",
        },
    }
}

/// Every item the grading can produce, so a document can declare them all
/// before anything has made one.
pub const ITEMS: &[&str] = &[
    "Power",
    "Water",
    "Coal",
    "Crude",
    "IronOre",
    "OrePowder",
    "Concentrate",
    "IronBillet",
    "IronStrip",
    "Gear",
    "Scrap",
    "Iron",
    "Slag",
    "LightFraction",
    "MiddleFraction",
    "HeavyFraction",
    "Heat",
    "Torque",
    "Stroke",
];

/// What an item is called in a sentence a player reads.
pub fn item_title(item: &str) -> &str {
    match item {
        "IronOre" => "iron ore",
        "OrePowder" => "ore powder",
        "Concentrate" => "concentrate",
        "IronBillet" => "iron billet",
        "IronStrip" => "iron strip",
        "LightFraction" => "light fraction",
        "MiddleFraction" => "middle fraction",
        "HeavyFraction" => "heavy fraction",
        "Power" => "power",
        "Gear" => "gears",
        "Water" => "water",
        "Coal" => "coal",
        "Crude" => "crude",
        "Slag" => "slag",
        "Scrap" => "scrap",
        other => other,
    }
}

/// A design, as the world sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Macro {
    pub name: String,
    /// One cycle, in world ticks: the orbit, read at a second a tick.
    pub cycle: Tick,
    pub takes: Vec<(String, Qty)>,
    pub gives: Vec<(String, Qty)>,
    /// Heat thrown away over one cycle, and the power made over the same one.
    /// Both are wanted by the efficiency goals, and neither is an item.
    pub wasted: u128,
    pub power: u128,
    /// Tiles the design occupies, and the world footprint that implies.
    pub design_w: u32,
    pub design_h: u32,
    pub w: i32,
    pub h: i32,
    pub components: usize,
    /// False when the design had not repeated itself before the search gave
    /// up, in which case the rates are the honest prefix average and the
    /// machine's inspector says so.
    pub settled: bool,
    pub transient: Tick,
}

impl Macro {
    /// The footprint a design of this size occupies in the world.
    pub fn footprint(design_w: u32, design_h: u32) -> (i32, i32) {
        let f = |n: u32| {
            let t = TILE_IN_DESIGN_TILES as u32;
            (n.div_ceil(t)).max(2) as i32
        };
        (f(design_w), f(design_h))
    }

    pub fn gives_of(&self, item: &str) -> Qty {
        self.gives.iter().find(|(i, _)| i == item).map(|(_, q)| *q).unwrap_or(0)
    }

    /// Steady output of one item, per second, for a panel.
    pub fn rate_of(&self, item: &str) -> f64 {
        self.gives_of(item) as f64 / super::as_secs(self.cycle).max(1e-9)
    }

    pub fn to_json(&self) -> Json {
        let amounts = |v: &[(String, Qty)]| {
            Json::Arr(
                v.iter()
                    .map(|(i, q)| {
                        Json::obj()
                            .set("item", i.clone())
                            .set("qty", Json::big(*q as u128))
                            .set("perSecond", *q as f64 / super::as_secs(self.cycle).max(1e-9))
                    })
                    .collect(),
            )
        };
        Json::obj()
            .set("name", self.name.clone())
            .set("cycle", self.cycle)
            .set("cycleSeconds", super::as_secs(self.cycle))
            .set("takes", amounts(&self.takes))
            .set("gives", amounts(&self.gives))
            .set("power", Json::big(self.power))
            .set("wasted", Json::big(self.wasted))
            .set("width", self.w as i64)
            .set("height", self.h as i64)
            .set("designWidth", self.design_w as i64)
            .set("designHeight", self.design_h as i64)
            .set("components", self.components as i64)
            .set("settled", self.settled)
            .set("transient", self.transient)
    }
}

/// Run the design until it repeats itself, and read the orbit as a recipe.
pub fn lower(d: &Design) -> Result<Macro, String> {
    if let Some(f) = d.check().first() {
        return Err(f.what.clone());
    }
    let c = orbit::compile(d)?;
    let r = eval::report(d, &c);
    let den = if c.settled() { c.period } else { c.searched.max(1) };
    Ok(of_report(d, &r, den))
}

/// The report, as a recipe. Split out so a caller that already has one -- the
/// inspector, which wants the design's own numbers as well -- does not compile
/// the same design twice.
pub fn of_report(d: &Design, r: &Report, den: Tick) -> Macro {
    let mut takes: Vec<(String, Qty)> = Vec::new();
    let mut gives: Vec<(String, Qty)> = Vec::new();
    for s in &r.takes {
        add(&mut takes, item_of(&s.what), s.rate.num, s.rate.den, den);
    }
    for s in &r.gives {
        add(&mut gives, item_of(&s.what), s.rate.num, s.rate.den, den);
    }
    // Electricity is already among the gives: `Totals::power` is a *mirror* of
    // the Power entries leaving through a boundary port, kept as its own
    // number because the first brief is written in megawatts. Adding it again
    // here would double every generator in the game, which is exactly what the
    // first version of this file did -- and what `the_primitive_cycle_keeps_
    // the_rate` caught, by checking the lowered rate against the orbit it came
    // from rather than against a plausible-looking figure.
    takes.sort();
    gives.sort();
    // The primitive cycle. An orbit is a fact about the machine's *internal*
    // state coming round again, and it can be sixty ticks long while every
    // flow across the boundary repeats every ten. The world only ever sees the
    // flows, so it is entitled to the shortest cycle that states them in whole
    // units -- and it wants it, because the alternative is a crusher that
    // swallows eighteen thousand water in one gulp and needs a bay the size of
    // a lake to be fed at all.
    //
    //   period 60, flows { 6000, 5610, 18672, 2244 }  ->  gcd 6
    //   cycle  10, flows { 1000,  935,  3112,  374 }
    //
    // The rate is identical, the arithmetic is exact, and the granularity is
    // the finest the orbit actually justifies.
    let mut g = den as u128;
    for (_, q) in takes.iter().chain(gives.iter()) {
        g = gcd(g, *q as u128);
    }
    let g = g.max(1);
    for (_, q) in takes.iter_mut().chain(gives.iter_mut()) {
        *q /= g as Qty;
    }
    let cycle = (den as u128 / g) as Tick;
    let (w, h) = Macro::footprint(r.width, r.height);
    Macro {
        name: d.name.clone(),
        cycle: cycle * DESIGN_TICK,
        takes,
        gives,
        wasted: r.wasted.num / g,
        power: r.power.num / g,
        design_w: r.width,
        design_h: r.height,
        w,
        h,
        components: r.components,
        settled: r.settled,
        transient: r.transient,
    }
}

fn gcd(a: u128, b: u128) -> u128 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// One stream, folded into the list at the common denominator.
///
/// Every rate in a report shares the orbit's denominator, so the rescale is
/// almost always a multiply by one. It is written out anyway: a silent
/// assumption about the shape of somebody else's struct is how exactness gets
/// lost.
fn add(into: &mut Vec<(String, Qty)>, item: &str, num: u128, den: u128, want: Tick) {
    if num == 0 {
        return;
    }
    let scaled = num.saturating_mul(want as u128) / den.max(1);
    let q = scaled.min(Qty::MAX as u128) as Qty;
    if q == 0 {
        return;
    }
    match into.iter_mut().find(|(i, _)| i == item) {
        Some((_, have)) => *have = have.saturating_add(q),
        None => into.push((item.to_string(), q)),
    }
}
