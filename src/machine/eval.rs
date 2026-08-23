//! What the design is worth, against one of four briefs.
//!
//! Experiment 06 had one brief and no score, on purpose: a single number would
//! immediately be maximised and the interesting part -- that a *compact* 100 MW
//! plant and a *clean* 100 MW plant are different machines -- would be gone by
//! the second attempt.
//!
//! Experiment 07 keeps that and adds the other half of the argument. One brief
//! proves a component set can answer one question. Four of them, answered by
//! the same thirty-eight components, is the only evidence that the vocabulary is
//! a vocabulary rather than a very elaborate way of writing `Boiler Mk2`:
//!
//! ```text
//!   power    100 MW from one fuel source        heat, fluid, gas, rotary, electrical
//!   crush    30/tick of 80%-pure ore powder     motor, gearbox, rotary, material
//!   distil   25 light and 40 middle per tick    heat, phase change, fluid separation
//!   gears    20 gears/tick from iron billet     material handling, forming, buffering
//! ```
//!
//! Every rate here is a rational, `n/d`, taken over one orbit rather than over
//! whatever window the player happens to be looking at. A machine with a period
//! of 47 has an average that no finite decimal states exactly, and rounding it
//! before comparing two designs is how you end up unable to explain why the
//! worse one won.

use super::design::Design;
use super::orbit::Compiled;
use super::parts::{self, Kind};
use super::sim::{FlowBig, Machine, Tick, Totals};
use super::stuff::{Stuff, Subst, FORM_GEAR, FORM_NAMES, SIZE_NAMES, SIZE_POWDER};
use crate::json::Json;

/// Experiment 06's brief, kept as the number it always was so that six designs
/// written against it are still judged by it.
pub const TARGET_MW: u128 = 100;

// ------------------------------------------------------------------ a brief

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Brief {
    #[default]
    Power,
    Crush,
    Distil,
    Gears,
}

pub const BRIEFS: [Brief; 4] = [Brief::Power, Brief::Crush, Brief::Distil, Brief::Gears];

/// One thing a brief asks the machine to produce, and what would count.
///
/// The properties are the point. "Iron Ore" is not a product; iron ore as
/// powder at 80% is. That is the whole thesis of `stuff` stated as an
/// acceptance test.
pub struct Target {
    pub label: &'static str,
    pub subst: Subst,
    pub per_tick: u128,
    pub min_purity: u8,
    pub max_temp: u8,
    pub form: Option<u8>,
    pub min_size: Option<u8>,
}

impl Target {
    fn accepts(&self, s: &Stuff) -> bool {
        s.subst == self.subst
            && s.q.purity >= self.min_purity
            && s.q.temp <= self.max_temp
            && self.form.map(|f| s.q.form == f).unwrap_or(true)
            && self.min_size.map(|z| s.q.size >= z).unwrap_or(true)
    }

    /// What the target asks for, in the words the inspector uses.
    pub fn wanted(&self) -> String {
        let mut bits = vec![self.subst.title().to_string()];
        if let Some(f) = self.form {
            bits.push(FORM_NAMES[(f as usize).min(4)].to_string());
        }
        if let Some(z) = self.min_size {
            bits.push(format!("{} or finer", SIZE_NAMES[(z as usize).min(3)]));
        }
        if self.min_purity > 0 {
            bits.push(format!("{}%+ pure", self.min_purity));
        }
        if self.max_temp < 9 {
            bits.push(format!("no hotter than band {}", self.max_temp));
        }
        bits.join(", ")
    }
}

const fn target(label: &'static str, subst: Subst, per_tick: u128) -> Target {
    Target { label, subst, per_tick, min_purity: 0, max_temp: 9, form: None, min_size: None }
}

static POWER: [Target; 1] = [target("electricity", Subst::Power, TARGET_MW)];
static CRUSH: [Target; 1] = [Target {
    label: "concentrate",
    subst: Subst::Ore,
    per_tick: 30,
    min_purity: 80,
    max_temp: 9,
    form: None,
    min_size: Some(SIZE_POWDER),
}];
static DISTIL: [Target; 2] = [
    Target {
        label: "light",
        subst: Subst::Light,
        per_tick: 25,
        min_purity: 0,
        // Condensed, not vapour. The condenser is not decoration.
        max_temp: 2,
        form: None,
        min_size: None,
    },
    target("middle", Subst::Middle, 40),
];
static GEARS: [Target; 1] = [Target {
    label: "gears",
    subst: Subst::Iron,
    per_tick: 20,
    min_purity: 0,
    max_temp: 9,
    form: Some(FORM_GEAR),
    min_size: None,
}];

impl Brief {
    pub fn tag(self) -> &'static str {
        match self {
            Brief::Power => "power",
            Brief::Crush => "crush",
            Brief::Distil => "distil",
            Brief::Gears => "gears",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Brief::Power => "Generate electricity",
            Brief::Crush => "Crush ore",
            Brief::Distil => "Distil mixed fluid",
            Brief::Gears => "Manufacture gears",
        }
    }

    /// The brief, in the sentence a player should be able to hold in their head.
    pub fn goal(self) -> &'static str {
        match self {
            Brief::Power => {
                "Produce at least 100 MW from one fuel source, on the smallest plot, \
                 with the least water and the least wasted heat."
            }
            Brief::Crush => {
                "Turn raw ore into at least 30/tick of 80%-pure powder, drawing as \
                 little from the grid as you can."
            }
            Brief::Distil => {
                "Split crude into at least 25/tick of condensed light and 40/tick of \
                 middle, without heating the sky."
            }
            Brief::Gears => {
                "Make at least 20 gears/tick out of iron billet, wasting as little \
                 metal and as little grid power as possible."
            }
        }
    }

    /// What it is testing, which is the reason there are four of these.
    pub fn tests(self) -> &'static str {
        match self {
            Brief::Power => "heat, fluid, gas, rotary, electrical",
            Brief::Crush => "motor, gearbox, rotary, material transformation",
            Brief::Distil => "heat, phase change, fluid separation",
            Brief::Gears => "material handling, forming, buffering",
        }
    }

    pub fn targets(self) -> &'static [Target] {
        match self {
            Brief::Power => &POWER,
            Brief::Crush => &CRUSH,
            Brief::Distil => &DISTIL,
            Brief::Gears => &GEARS,
        }
    }

    /// Only the first brief cares how many fires are burning. It is the one
    /// sentence experiment 06 wrote into its brief, and dropping it would
    /// quietly make its six designs easier.
    pub fn one_source(self) -> bool {
        self == Brief::Power
    }

    pub fn by_tag(tag: &str) -> Option<Brief> {
        BRIEFS.iter().copied().find(|b| b.tag() == tag)
    }
}

// ------------------------------------------------------------------- rates

#[derive(Clone, Copy)]
pub struct Rate {
    pub num: u128,
    pub den: u128,
}

impl Rate {
    pub fn new(num: u128, den: u128) -> Rate {
        Rate { num, den: den.max(1) }
    }
    pub fn value(&self) -> f64 {
        self.num as f64 / self.den as f64
    }
    /// This rate per unit of output -- the only fair way to compare a 104 MW
    /// machine with a 260 MW one.
    ///
    /// `None` when there is no output. A machine that produces nothing does not
    /// have a large footprint per megawatt; it has no answer to the question,
    /// and saying 7.8e10 instead is the sort of confident nonsense this crate
    /// exists not to print. It is also not representable: JSON has no infinity,
    /// so an infinity sent to a browser arrives as `null` and the first thing
    /// that treats it as a number throws.
    pub fn per(&self, other: &Rate) -> Option<f64> {
        if other.num == 0 {
            return None;
        }
        Some((self.num as f64 / self.den as f64) / (other.num as f64 / other.den as f64))
    }
    fn to_json(self) -> Json {
        Json::obj()
            .set("num", Json::big(self.num))
            .set("den", Json::big(self.den))
            .set("value", Json::Real(self.value()))
    }
}

/// An optional number, on the wire.
fn opt(v: Option<f64>) -> Json {
    match v {
        Some(x) if x.is_finite() => Json::Real(x),
        _ => Json::Null,
    }
}

fn per_out(value: f64, out: &Rate) -> Json {
    opt(if out.num == 0 { None } else { Some(value / out.value()) })
}

/// One line of the scoreboard: something crossing the machine's boundary.
pub struct Stream {
    pub what: Stuff,
    pub rate: Rate,
}

fn streams(f: &FlowBig, ticks: u128) -> Vec<Stream> {
    let mut v: Vec<Stream> =
        f.iter().map(|(s, n)| Stream { what: *s, rate: Rate::new(*n, ticks) }).collect();
    v.sort_by(|a, b| b.rate.num.cmp(&a.rate.num));
    v
}

fn streams_json(v: &[Stream]) -> Json {
    Json::Arr(
        v.iter()
            .map(|s| {
                Json::obj()
                    .set("what", s.what.to_json())
                    .set("label", s.what.label())
                    .set("rate", s.rate.to_json())
            })
            .collect(),
    )
}

// ------------------------------------------------------------------ report

/// How one target did.
pub struct Scored {
    pub label: &'static str,
    pub wanted: String,
    pub need: u128,
    pub got: Rate,
    pub met: bool,
}

pub struct Report {
    pub name: String,
    pub brief: Brief,
    /// Steady-state rates, over exactly one orbit.
    pub power: Rate,
    pub fuel: Rate,
    pub water: Rate,
    pub grid: Rate,
    pub wasted: Rate,
    pub vented: Rate,
    pub util: Rate,
    /// What the brief asked for, and what it got.
    pub scored: Vec<Scored>,
    /// Everything crossing the boundary, whether the brief asked for it or not.
    pub takes: Vec<Stream>,
    pub gives: Vec<Stream>,
    pub loses: Vec<Stream>,
    /// Plot, in tiles.
    pub width: u32,
    pub height: u32,
    /// Tiles actually covered by a component.
    pub tiles: u32,
    pub components: usize,
    pub sources: usize,
    pub transient: Tick,
    pub period: Tick,
    pub settled: bool,
    /// Why the design does not meet the brief, if it does not.
    pub failings: Vec<String>,
}

pub fn report(d: &Design, c: &Compiled) -> Report {
    let (w, h, tiles) = d.footprint();
    let n = d.units.len().max(1) as u128;
    let (p, ticks) = if c.period > 0 {
        (c.orbit.clone(), c.period as u128)
    } else {
        (c.cum.last().cloned().unwrap_or_default(), c.searched.max(1) as u128)
    };

    let power = Rate::new(p.power, ticks);
    let fuel = Rate::new(p.fuel(), ticks);
    let water = Rate::new(p.water(), ticks);
    let grid = Rate::new(p.grid(), ticks);
    let wasted = Rate::new(p.heat_wasted, ticks);
    let vented = Rate::new(p.vented(), ticks);
    // util_sum is per mille per component per tick; a percentage of the whole
    // machine is that divided by components and by ten.
    let util = Rate::new(p.util_sum, ticks * n * 10);

    let mut failings = Vec::new();
    let scored: Vec<Scored> = d
        .brief
        .targets()
        .iter()
        .map(|t| {
            let got: u128 =
                p.gave.iter().filter(|(s, _)| t.accepts(s)).map(|(_, v)| *v).sum();
            let rate = Rate::new(got, ticks);
            let met = got >= t.per_tick * ticks;
            if !met {
                failings.push(format!(
                    "{:.2}/tick of {} is short of {} by {:.2}",
                    rate.value(),
                    t.wanted(),
                    t.per_tick,
                    t.per_tick as f64 - rate.value()
                ));
                // The near miss worth naming: the machine is making the right
                // substance and it is not good enough, which is a different
                // problem from making none of it.
                let any: u128 = p
                    .gave
                    .iter()
                    .filter(|(s, _)| s.subst == t.subst && !t.accepts(s))
                    .map(|(_, v)| *v)
                    .sum();
                if any > 0 {
                    if let Some((s, _)) =
                        p.gave.iter().find(|(s, _)| s.subst == t.subst && !t.accepts(s))
                    {
                        failings.push(format!(
                            "  it is leaving as {} -- {:.2}/tick of it",
                            s.label(),
                            Rate::new(any, ticks).value()
                        ));
                    }
                }
            }
            Scored { label: t.label, wanted: t.wanted(), need: t.per_tick, got: rate, met }
        })
        .collect();

    let sources = d.count_of(Kind::Reactor) + d.count_of(Kind::Burner);
    if d.brief.one_source() && sources != 1 {
        failings.push(format!("the brief is one fuel source, and this design has {sources}"));
    }
    if !c.settled() {
        failings.push(format!(
            "the machine had still not repeated itself after {} ticks, so its \
             steady state is an estimate rather than a fact",
            c.searched
        ));
    }

    Report {
        name: d.name.clone(),
        brief: d.brief,
        power,
        fuel,
        water,
        grid,
        wasted,
        vented,
        util,
        scored,
        takes: streams(&p.took, ticks),
        gives: streams(&p.gave, ticks),
        loses: streams(&p.lost, ticks),
        width: w,
        height: h,
        tiles,
        components: d.units.len(),
        sources,
        transient: c.transient,
        period: c.period,
        settled: c.settled(),
        failings,
    }
}

impl Report {
    pub fn met(&self) -> bool {
        self.failings.is_empty()
    }

    pub fn area(&self) -> u32 {
        self.width * self.height
    }

    /// How much of the plot is machine rather than air.
    pub fn density(&self) -> f64 {
        if self.area() == 0 {
            0.0
        } else {
            self.tiles as f64 / self.area() as f64
        }
    }

    /// The rate everything else is judged per unit of: the first target's
    /// output. For the power brief that is megawatts, which is what experiment
    /// 06 divided by.
    pub fn headline(&self) -> Rate {
        self.scored.first().map(|s| s.got).unwrap_or(Rate::new(0, 1))
    }

    /// The unit the headline is counted in.
    pub fn headline_unit(&self) -> &'static str {
        if self.brief == Brief::Power {
            "MW"
        } else {
            "units"
        }
    }

    pub fn to_json(&self) -> Json {
        let out = self.headline();
        Json::obj()
            .set("name", self.name.clone())
            .set("brief", self.brief.tag())
            .set("briefTitle", self.brief.title())
            .set("goal", self.brief.goal())
            .set("tests", self.brief.tests())
            .set("targetMw", Json::big(TARGET_MW))
            .set("met", self.met())
            .set("headline", out.to_json())
            .set("headlineUnit", self.headline_unit())
            .set(
                "targets",
                Json::Arr(
                    self.scored
                        .iter()
                        .map(|s| {
                            Json::obj()
                                .set("label", s.label)
                                .set("wanted", s.wanted.clone())
                                .set("need", Json::big(s.need))
                                .set("got", s.got.to_json())
                                .set("met", s.met)
                        })
                        .collect(),
                ),
            )
            .set("power", self.power.to_json())
            .set("fuel", self.fuel.to_json())
            .set("water", self.water.to_json())
            .set("grid", self.grid.to_json())
            .set("wasted", self.wasted.to_json())
            .set("vented", self.vented.to_json())
            .set("utilisation", self.util.to_json())
            .set("takes", streams_json(&self.takes))
            .set("gives", streams_json(&self.gives))
            .set("loses", streams_json(&self.loses))
            .set("width", self.width as i64)
            .set("height", self.height as i64)
            .set("area", self.area() as i64)
            .set("tiles", self.tiles as i64)
            .set("density", Json::Real(self.density()))
            .set("components", self.components as i64)
            .set("sources", self.sources as i64)
            .set("transient", self.transient as i64)
            .set("period", self.period as i64)
            .set("settled", self.settled)
            .set(
                "per",
                Json::obj()
                    .set("areaPerOut", per_out(self.area() as f64, &out))
                    .set("tilesPerOut", per_out(self.tiles as f64, &out))
                    .set("fuelPerOut", opt(self.fuel.per(&out)))
                    .set("waterPerOut", opt(self.water.per(&out)))
                    .set("gridPerOut", opt(self.grid.per(&out)))
                    .set("wastedPerOut", opt(self.wasted.per(&out))),
            )
            .set("failings", Json::arr(self.failings.clone()))
    }

    /// The same thing, for a terminal.
    pub fn text(&self) -> String {
        let out = self.headline();
        let mut s = String::new();
        s.push_str(&format!("{}   [{}]\n", self.name, self.brief.title()));
        for sc in &self.scored {
            s.push_str(&format!(
                "  {:<20}{:>12.2} /tick   of {} {}{}\n",
                sc.label,
                sc.got.value(),
                sc.need,
                sc.wanted,
                if sc.met { "   MET" } else { "" }
            ));
        }
        s.push_str(&format!("  {:<20}{:>12.2} MW\n", "electrical out", self.power.value()));
        if self.grid.num > 0 {
            s.push_str(&format!("  {:<20}{:>12.2} MW\n", "grid draw", self.grid.value()));
        }
        s.push_str(&format!("  {:<20}{:>12.2} /tick\n", "fuel", self.fuel.value()));
        s.push_str(&format!("  {:<20}{:>12.2} /tick\n", "water", self.water.value()));
        s.push_str(&format!("  {:<20}{:>12.2} /tick\n", "heat wasted", self.wasted.value()));
        s.push_str(&format!("  {:<20}{:>12.2} /tick\n", "matter thrown away", self.vented.value()));
        s.push_str(&format!(
            "  {:<20}{:>12} tiles   {} x {}, {:.0}% covered\n",
            "footprint",
            self.area(),
            self.width,
            self.height,
            self.density() * 100.0
        ));
        s.push_str(&format!("  {:<20}{:>12}\n", "components", self.components));
        s.push_str(&format!("  {:<20}{:>11.1}%\n", "utilisation", self.util.value()));
        if self.settled {
            s.push_str(&format!(
                "  {:<20}{:>12} ticks, then a period of {}\n",
                "transient", self.transient, self.period
            ));
        } else {
            s.push_str(&format!("  {:<20}{:>12}\n", "transient", "never settled"));
        }
        let show = |v: Option<f64>| match v {
            Some(x) => format!("{x:.2}"),
            None => "--".to_string(),
        };
        let area_per = if out.num == 0 { None } else { Some(self.area() as f64 / out.value()) };
        s.push_str(&format!(
            "  {:<20}{:>12} tiles/out   {} water/out   {} wasted/out\n",
            "per unit made",
            show(area_per),
            show(self.water.per(&out)),
            show(self.wasted.per(&out))
        ));
        if !self.gives.is_empty() {
            s.push_str("  leaving:\n");
            for g in &self.gives {
                s.push_str(&format!("    {:<40}{:>10.2} /tick\n", g.what.label(), g.rate.value()));
            }
        }
        for f in &self.failings {
            s.push_str(&format!("  ! {f}\n"));
        }
        s
    }
}

/// What the machine looks like from outside: the thing a factory would place.
///
/// This is the payoff of compiling an orbit. A design that has settled
/// advertises exact external rates and an exact repeat length, and nothing that
/// places it ever has to materialise a heat exchanger again.
pub fn macro_machine(d: &Design, c: &Compiled, r: &Report) -> Json {
    let stream = |v: &[Stream]| {
        Json::Arr(
            v.iter()
                .map(|s| {
                    Json::obj()
                        .set("what", s.what.label())
                        .set("stuff", s.what.to_json())
                        .set("rate", s.rate.to_json())
                })
                .collect(),
        )
    };
    // The internal state a resumed instance would have to be given back. It is
    // the same byte string `orbit` keys on, so the count is not an estimate.
    let internal = Machine::new(d).map(|m| m.key().len()).unwrap_or(0);
    let mut waste = r.loses.iter().map(|s| Stream { what: s.what, rate: s.rate }).collect::<Vec<_>>();
    if r.wasted.num > 0 {
        waste.push(Stream {
            what: Stuff::fresh(Subst::Heat),
            rate: r.wasted,
        });
    }
    Json::obj()
        .set("name", d.name.clone())
        .set("brief", d.brief.tag())
        .set("externalInputs", stream(&r.takes))
        .set("externalOutputs", stream(&r.gives))
        .set("waste", stream(&waste))
        .set("footprint", format!("{} x {}", r.width, r.height))
        .set("internalComponents", d.units.len() as i64)
        .set("internalStateBytes", internal as i64)
        .set("transient", c.transient as i64)
        .set("periodicOrbit", c.period as i64)
        .set("settled", c.settled())
        .set(
            "note",
            if c.settled() {
                format!(
                    "{} ticks of startup, then the same {} ticks forever",
                    c.transient, c.period
                )
            } else {
                "still transient at the end of the search".to_string()
            },
        )
}

/// Which substance a brief's headline is counted in, so that the orbit strip
/// draws the thing the player is being judged on rather than always drawing
/// megawatts.
pub fn headline_subst(b: Brief) -> Subst {
    b.targets().first().map(|t| t.subst).unwrap_or(Subst::Power)
}

/// Every brief, for a client that wants to offer them.
pub fn briefs() -> Json {
    Json::Arr(
        BRIEFS
            .iter()
            .map(|b| {
                Json::obj()
                    .set("tag", b.tag())
                    .set("title", b.title())
                    .set("goal", b.goal())
                    .set("tests", b.tests())
                    .set(
                        "targets",
                        Json::Arr(
                            b.targets()
                                .iter()
                                .map(|t| {
                                    Json::obj()
                                        .set("label", t.label)
                                        .set("need", Json::big(t.per_tick))
                                        .set("wanted", t.wanted())
                                })
                                .collect(),
                        ),
                    )
            })
            .collect(),
    )
}

/// The catalogue's numbers, for a panel that wants to show the player what they
/// are working with without hard-coding it in JavaScript.
pub fn constants() -> Json {
    Json::obj()
        .set("warmup", parts::WARMUP as i64)
        .set("reactorHeat", parts::REACTOR_HEAT as i64)
        .set("reactorFuel", parts::REACTOR_FUEL as i64)
        .set("minThrottle", parts::MIN_THROTTLE as i64)
        .set("pipeLossPct", parts::PIPE_LOSS_PCT as i64)
        .set("reach", parts::REACH as i64)
        .set("turbineMin", parts::TURBINE_MIN as i64)
        .set("spinMax", parts::SPIN_MAX as i64)
        .set("spinUp", parts::SPIN_UP as i64)
        .set("spinDown", parts::SPIN_DOWN as i64)
        .set("turbineEff", parts::TURBINE_EFF as i64)
        .set("generatorEff", parts::GENERATOR_EFF as i64)
        .set("driveSpeed", parts::DRIVE_SPEED as i64)
        .set("speedMax", super::stuff::SPEED_MAX as i64)
        .set("tempMax", super::stuff::TEMP_MAX as i64)
        .set("tankCap", parts::part(Kind::Tank).ports[0].cap as i64)
        .set("columnStages", parts::COLUMN_MAX_STAGES as i64)
        .set("targetMw", Json::big(TARGET_MW))
        .set("tempNames", Json::arr(super::stuff::TEMP_NAMES.to_vec()))
        .set("sizeNames", Json::arr(super::stuff::SIZE_NAMES.to_vec()))
        .set("formNames", Json::arr(super::stuff::FORM_NAMES.to_vec()))
}

/// How many of the shipped designs each component appears in, and how many
/// distinct briefs.
///
/// This is the experiment's own acceptance test, and it is deliberately a
/// number rather than an opinion: the note that asked for experiment 07 said
/// that if the same motor, pump, buffer and shaft turn up across several
/// designs the primitives are good, and that if every challenge needs ten
/// bespoke components used nowhere else the abstraction is wrong. So count
/// them.
pub struct Uses {
    pub kind: Kind,
    pub designs: usize,
    pub briefs: Vec<Brief>,
    pub placed: usize,
}

pub fn reuse(designs: &[Design]) -> Vec<Uses> {
    parts::KINDS
        .iter()
        .map(|&k| {
            let mut briefs: Vec<Brief> = Vec::new();
            let mut in_designs = 0;
            let mut placed = 0;
            for d in designs {
                let n = d.count_of(k);
                if n > 0 {
                    in_designs += 1;
                    placed += n;
                    if !briefs.contains(&d.brief) {
                        briefs.push(d.brief);
                    }
                }
            }
            Uses { kind: k, designs: in_designs, briefs, placed }
        })
        .collect()
}


/// The totals a caller wants without knowing about maps.
pub fn totals_json(t: &Totals) -> Json {
    let flow = |f: &FlowBig| {
        Json::Arr(
            f.iter()
                .map(|(s, n)| {
                    Json::obj().set("what", s.label()).set("qty", Json::big(*n))
                })
                .collect(),
        )
    };
    Json::obj()
        .set("ticks", Json::big(t.ticks))
        .set("power", Json::big(t.power))
        .set("fuel", Json::big(t.fuel()))
        .set("water", Json::big(t.water()))
        .set("grid", Json::big(t.grid()))
        .set("heatWasted", Json::big(t.heat_wasted))
        .set("lost", Json::big(t.vented()))
        .set("took", flow(&t.took))
        .set("gave", flow(&t.gave))
}
