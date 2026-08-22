//! What the design is worth.
//!
//! The brief is one sentence with four halves:
//!
//! > Produce at least 100 MW from one fuel source while minimising footprint,
//! > water use, and wasted heat.
//!
//! so there is deliberately no score. A single number would immediately be
//! maximised and the interesting part -- that a *compact* 100 MW plant and a
//! *clean* 100 MW plant are different machines -- would be gone by the second
//! attempt. What this module produces is a row of honest numbers, the target
//! marked met or not, and each cost also expressed per megawatt, which is the
//! form in which two designs of different sizes can actually be compared.
//!
//! Every rate here is a rational, `n/d`, taken over one orbit rather than over
//! whatever window the player happens to be looking at. A machine with a period
//! of 47 has an average that no finite decimal states exactly, and rounding it
//! before comparing two designs is how you end up unable to explain why the
//! worse one won.

use super::design::Design;
use super::orbit::Compiled;
use super::parts::{self, Kind};
use super::sim::{Machine, Tick};
use crate::json::Json;

/// The brief.
pub const TARGET_MW: u128 = 100;

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
    /// This rate per megawatt of output -- the only fair way to compare a
    /// 104 MW machine with a 260 MW one.
    ///
    /// `None` when there are no megawatts. A machine that produces nothing does
    /// not have a large footprint per megawatt; it has no answer to the
    /// question, and saying 7.8e10 instead is the sort of confident nonsense
    /// this crate exists not to print. It is also not representable: JSON has
    /// no infinity, so an infinity sent to a browser arrives as `null` and the
    /// first thing that treats it as a number throws.
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
///
/// JSON has no infinity and no NaN, so a figure that does not exist has to be
/// `null` -- not a float that a client will happily call `.toFixed()` on.
fn opt(v: Option<f64>) -> Json {
    match v {
        Some(x) if x.is_finite() => Json::Real(x),
        _ => Json::Null,
    }
}

fn per_mw(value: f64, mw: &Rate) -> Json {
    opt(if mw.num == 0 { None } else { Some(value / mw.value()) })
}

pub struct Report {
    pub name: String,
    /// Steady-state rates, over exactly one orbit.
    pub power: Rate,
    pub fuel: Rate,
    pub water: Rate,
    pub wasted: Rate,
    pub vented: Rate,
    pub util: Rate,
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
        (c.orbit, c.period as u128)
    } else {
        (*c.cum.last().unwrap_or(&Default::default()), c.searched.max(1) as u128)
    };

    let power = Rate::new(p.power, ticks);
    let fuel = Rate::new(p.fuel, ticks);
    let water = Rate::new(p.water, ticks);
    let wasted = Rate::new(p.heat_wasted, ticks);
    let vented = Rate::new(p.steam_vented, ticks);
    // util_sum is per mille per component per tick; a percentage of the whole
    // machine is that divided by components and by ten.
    let util = Rate::new(p.util_sum, ticks * n * 10);

    let sources = d.count_of(Kind::Reactor);
    let mut failings = Vec::new();
    if power.num * 1 < TARGET_MW * power.den {
        failings.push(format!(
            "{:.2} MW is short of {TARGET_MW} MW by {:.2}",
            power.value(),
            TARGET_MW as f64 - power.value()
        ));
    }
    if sources != 1 {
        failings.push(format!(
            "the brief is one fuel source, and this design has {sources}"
        ));
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
        power,
        fuel,
        water,
        wasted,
        vented,
        util,
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

    pub fn to_json(&self) -> Json {
        let mw = self.power;
        Json::obj()
            .set("name", self.name.clone())
            .set("targetMw", Json::big(TARGET_MW))
            .set("met", self.met())
            .set("power", mw.to_json())
            .set("fuel", self.fuel.to_json())
            .set("water", self.water.to_json())
            .set("wasted", self.wasted.to_json())
            .set("vented", self.vented.to_json())
            .set("utilisation", self.util.to_json())
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
                    .set("areaPerMw", per_mw(self.area() as f64, &mw))
                    .set("tilesPerMw", per_mw(self.tiles as f64, &mw))
                    .set("fuelPerMw", opt(self.fuel.per(&mw)))
                    .set("waterPerMw", opt(self.water.per(&mw)))
                    .set("wastedPerMw", opt(self.wasted.per(&mw))),
            )
            .set("failings", Json::arr(self.failings.clone()))
    }

    /// The same thing, for a terminal.
    pub fn text(&self) -> String {
        let mw = self.power;
        let mut s = String::new();
        s.push_str(&format!("{}\n", self.name));
        s.push_str(&format!(
            "  {:<20}{:>12.2} MW{}\n",
            "electrical output",
            mw.value(),
            if self.met() { "   TARGET MET" } else { "" }
        ));
        s.push_str(&format!("  {:<20}{:>12.2} /tick\n", "fuel", self.fuel.value()));
        s.push_str(&format!("  {:<20}{:>12.2} /tick\n", "water", self.water.value()));
        s.push_str(&format!("  {:<20}{:>12.2} /tick\n", "heat wasted", self.wasted.value()));
        s.push_str(&format!("  {:<20}{:>12.2} /tick\n", "steam vented", self.vented.value()));
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
        let area_per = if mw.num == 0 { None } else { Some(self.area() as f64 / mw.value()) };
        s.push_str(&format!(
            "  {:<20}{:>12} tiles/MW   {} water/MW   {} wasted/MW\n",
            "per megawatt",
            show(area_per),
            show(self.water.per(&mw)),
            show(self.wasted.per(&mw))
        ));
        for f in &self.failings {
            s.push_str(&format!("  ! {f}\n"));
        }
        s
    }
}

/// What the machine looks like from outside: the thing a factory would place.
///
/// This is the payoff of compiling an orbit. A design that has settled advertises
/// exact external rates and an exact repeat length, and nothing that places it
/// ever has to materialise a heat exchanger again.
pub fn macro_machine(d: &Design, c: &Compiled, r: &Report) -> Json {
    let mut inputs = Vec::new();
    if r.fuel.num > 0 {
        inputs.push(("Fuel", r.fuel));
    }
    if r.water.num > 0 {
        inputs.push(("Water", r.water));
    }
    let mut outputs = vec![("Electricity", r.power)];
    if r.wasted.num > 0 {
        outputs.push(("WasteHeat", r.wasted));
    }
    if r.vented.num > 0 {
        outputs.push(("WasteSteam", r.vented));
    }
    let stream = |v: &Vec<(&str, Rate)>| {
        Json::Arr(
            v.iter()
                .map(|(n, r)| {
                    Json::obj().set("what", *n).set("rate", r.to_json())
                })
                .collect(),
        )
    };
    // The internal state a resumed instance would have to be given back. It is
    // the same byte string `orbit` keys on, so the count is not an estimate.
    let internal = Machine::new(d).map(|m| m.key().len()).unwrap_or(0);
    Json::obj()
        .set("name", d.name.clone())
        .set("externalInputs", stream(&inputs))
        .set("externalOutputs", stream(&outputs))
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
        .set("targetMw", Json::big(TARGET_MW))
}
