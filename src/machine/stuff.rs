//! What a connection carries: a domain, a substance, and a handful of
//! properties.
//!
//! Experiment 06 had five port types and a connection carried a number. That
//! was enough for one power plant and not enough for anything else, because a
//! machine that only ever moves *amounts* can never answer the question
//! experiment 07 is about:
//!
//! > the recipe stays `Iron -> Gear`; the machine that performs it is where the
//! > complexity lives.
//!
//! For that, the thing on the wire has to be able to change without becoming a
//! new item. So a quantity here is a `Stuff` -- a substance plus five small
//! properties -- and a component's job is to *modify* it:
//!
//! ```text
//!   crusher     size    lump      -> crushed
//!   mill        size    crushed   -> powder
//!   separator   purity  40%       -> 82%
//!   furnace     temp    ambient   -> molten
//!   rolling     form    billet    -> strip
//!   press       form    strip     -> gear
//! ```
//!
//! The outer game still has one item called Iron Ore. Nothing here needs
//! `CrushedIronOre`, `FineIronOre` or `SlightlyMoistFineIronOre` to exist as
//! separate things to put in a chest.
//!
//! # Why phase is not a property
//!
//! It is a *domain*. Water in `fluid` boiled by an exchanger comes out in
//! `gas`; iron in `material` melted by a furnace comes out in `fluid`. That
//! makes a phase change something you can see on the canvas -- the wire changes
//! colour and will not plug into what it plugged into before -- rather than a
//! number inside a box. It also means "steam" is not a port type. Steam is
//! water, in gas, at a temperature.
//!
//! # Why every property is a small integer
//!
//! Because `orbit` compiles a design by watching for its state to repeat, and
//! a state that contains a float repeats approximately, which is to say never.
//! Temperature is a band, purity is a percent, size is one of four words. The
//! whole of a stuff is six bytes, and two stuffs are equal or they are not.

use std::fmt;

// ------------------------------------------------------------------ domains

/// What a wire carries. Two ports may be connected only if these match.
///
/// Seven, and deliberately not eight: the note that started this experiment
/// listed `control` as a possible later domain, and control here is done with
/// thresholds on the component that needs them rather than with a signal wire,
/// so there is nothing for an eighth domain to carry yet.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Domain {
    Material,
    Fluid,
    Gas,
    Heat,
    Rotary,
    Mech,
    Electrical,
}

pub const DOMAINS: [Domain; 7] = [
    Domain::Material,
    Domain::Fluid,
    Domain::Gas,
    Domain::Heat,
    Domain::Rotary,
    Domain::Mech,
    Domain::Electrical,
];

impl Domain {
    pub fn tag(self) -> &'static str {
        match self {
            Domain::Material => "material",
            Domain::Fluid => "fluid",
            Domain::Gas => "gas",
            Domain::Heat => "heat",
            Domain::Rotary => "rotary",
            Domain::Mech => "mech",
            Domain::Electrical => "electrical",
        }
    }

    /// The unit a number in this domain is counted in, for a panel that has to
    /// print one.
    pub fn unit(self) -> &'static str {
        match self {
            Domain::Material | Domain::Fluid | Domain::Gas => "units",
            Domain::Heat => "heat",
            Domain::Rotary => "rotary",
            Domain::Mech => "strokes",
            Domain::Electrical => "MW",
        }
    }

    /// Whether stuff in this domain is matter that has to be accounted for.
    /// Energy domains are conversions and are allowed to lose to efficiency;
    /// matter is not allowed to evaporate quietly.
    pub fn is_matter(self) -> bool {
        matches!(self, Domain::Material | Domain::Fluid | Domain::Gas)
    }
}

impl fmt::Display for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

// --------------------------------------------------------------- substances

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Subst {
    /// The energy domains each have exactly one substance, so that a buffer is
    /// one kind of thing whatever it holds and the whole machine is one model.
    Heat,
    Torque,
    Stroke,
    Power,
    // matter
    Water,
    Coal,
    Ore,
    Iron,
    Slag,
    Crude,
    Light,
    Middle,
    Heavy,
}

pub const SUBSTS: [Subst; 13] = [
    Subst::Heat,
    Subst::Torque,
    Subst::Stroke,
    Subst::Power,
    Subst::Water,
    Subst::Coal,
    Subst::Ore,
    Subst::Iron,
    Subst::Slag,
    Subst::Crude,
    Subst::Light,
    Subst::Middle,
    Subst::Heavy,
];

/// The substances a source can be told to draw. Everything else is something
/// a component made.
pub const SOURCES: [Subst; 5] =
    [Subst::Water, Subst::Coal, Subst::Ore, Subst::Iron, Subst::Crude];

struct SubstDef {
    tag: &'static str,
    title: &'static str,
    /// Where it lives when nothing has changed it.
    home: Domain,
    /// How hard it is to break. A crusher refuses above its rating.
    hardness: u8,
    /// The band at which a solid becomes a liquid. 0 means it never does.
    melt: u8,
    /// The band at which a liquid becomes a gas. 0 means it never does.
    boil: u8,
    /// What comes out of a fresh source of it.
    fresh: Qual,
}

const fn q(temp: u8, size: u8, purity: u8, form: u8) -> Qual {
    Qual { temp, size, purity, form, speed: 0 }
}

static SUBSTS_DEF: [SubstDef; 13] = [
    SubstDef { tag: "heat", title: "Heat", home: Domain::Heat, hardness: 0, melt: 0, boil: 0,
               fresh: q(0, 0, 100, 0) },
    SubstDef { tag: "torque", title: "Rotary", home: Domain::Rotary, hardness: 0, melt: 0, boil: 0,
               fresh: Qual { temp: 0, size: 0, purity: 100, form: 0, speed: 5 } },
    SubstDef { tag: "stroke", title: "Stroke", home: Domain::Mech, hardness: 0, melt: 0, boil: 0,
               fresh: q(0, 0, 100, 0) },
    SubstDef { tag: "power", title: "Electricity", home: Domain::Electrical, hardness: 0, melt: 0, boil: 0,
               fresh: q(0, 0, 100, 0) },
    SubstDef { tag: "water", title: "Water", home: Domain::Fluid, hardness: 0, melt: 0, boil: 2,
               fresh: q(0, 0, 100, 0) },
    SubstDef { tag: "coal", title: "Coal", home: Domain::Material, hardness: 2, melt: 0, boil: 0,
               fresh: q(0, SIZE_LUMP, 90, 0) },
    SubstDef { tag: "ore", title: "Iron Ore", home: Domain::Material, hardness: 6, melt: 8, boil: 0,
               fresh: q(0, SIZE_LUMP, 40, 0) },
    SubstDef { tag: "iron", title: "Iron", home: Domain::Material, hardness: 4, melt: 7, boil: 0,
               fresh: q(0, SIZE_LUMP, 96, FORM_BILLET) },
    SubstDef { tag: "slag", title: "Slag", home: Domain::Material, hardness: 5, melt: 8, boil: 0,
               fresh: q(0, SIZE_POWDER, 10, 0) },
    SubstDef { tag: "crude", title: "Crude", home: Domain::Fluid, hardness: 0, melt: 0, boil: 6,
               fresh: q(0, 0, 100, 0) },
    SubstDef { tag: "light", title: "Light Fraction", home: Domain::Fluid, hardness: 0, melt: 0, boil: 3,
               fresh: q(0, 0, 100, 0) },
    SubstDef { tag: "middle", title: "Middle Fraction", home: Domain::Fluid, hardness: 0, melt: 0, boil: 5,
               fresh: q(0, 0, 100, 0) },
    SubstDef { tag: "heavy", title: "Heavy Fraction", home: Domain::Fluid, hardness: 0, melt: 0, boil: 8,
               fresh: q(0, 0, 100, 0) },
];

impl Subst {
    fn def(self) -> &'static SubstDef {
        &SUBSTS_DEF[self as usize]
    }
    pub fn tag(self) -> &'static str {
        self.def().tag
    }
    pub fn title(self) -> &'static str {
        self.def().title
    }
    pub fn home(self) -> Domain {
        self.def().home
    }
    pub fn hardness(self) -> u8 {
        self.def().hardness
    }
    pub fn melt(self) -> u8 {
        self.def().melt
    }
    pub fn boil(self) -> u8 {
        self.def().boil
    }
    pub fn is_matter(self) -> bool {
        self.home().is_matter()
    }
    pub fn by_tag(tag: &str) -> Option<Subst> {
        SUBSTS.iter().copied().find(|s| s.tag() == tag)
    }
}

impl fmt::Display for Subst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.title())
    }
}

// --------------------------------------------------------------- properties

/// The hottest band anything reaches. Bands rather than degrees, because a
/// band is a fact a state key can be equal to.
pub const TEMP_MAX: u8 = 9;

pub const TEMP_NAMES: [&str; 10] = [
    "ambient", "warm", "hot", "very hot", "scorching", "red", "orange", "white",
    "molten", "furnace",
];

pub const SIZE_LUMP: u8 = 0;
pub const SIZE_COARSE: u8 = 1;
pub const SIZE_CRUSHED: u8 = 2;
pub const SIZE_POWDER: u8 = 3;
pub const SIZE_NAMES: [&str; 4] = ["lump", "coarse", "crushed", "powder"];

pub const FORM_RAW: u8 = 0;
pub const FORM_BILLET: u8 = 1;
pub const FORM_STRIP: u8 = 2;
pub const FORM_GEAR: u8 = 3;
pub const FORM_SCRAP: u8 = 4;
pub const FORM_NAMES: [&str; 5] = ["raw", "billet", "strip", "gear", "scrap"];

/// The most a shaft turns. Like temperature, a band: what matters is that a
/// crusher wants a low one and a mill wants a high one, not radians per second.
pub const SPEED_MAX: u8 = 9;

/// The five properties a stuff carries. Which of them mean anything depends on
/// the domain -- `speed` is only rotary, `size` is only material -- and the
/// ones that do not are simply never read.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Default)]
pub struct Qual {
    /// Band, `0..=TEMP_MAX`.
    pub temp: u8,
    /// Particle size, `SIZE_*`. Bigger is finer.
    pub size: u8,
    /// Percent.
    pub purity: u8,
    /// Shape, `FORM_*`.
    pub form: u8,
    /// Rotary only, `0..=SPEED_MAX`.
    pub speed: u8,
}

/// A substance with its properties: the thing a wire actually carries.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Stuff {
    pub subst: Subst,
    pub q: Qual,
}

impl Stuff {
    /// Straight out of a source, before anything has happened to it.
    pub fn fresh(subst: Subst) -> Stuff {
        Stuff { subst, q: subst.def().fresh }
    }

    pub fn with(subst: Subst, q: Qual) -> Stuff {
        Stuff { subst, q }
    }

    pub fn domain(&self) -> Domain {
        self.subst.home()
    }

    pub fn hardness(&self) -> u8 {
        self.subst.hardness()
    }

    /// Two quantities of the same substance, poured into the same buffer.
    ///
    /// Properties come out weighted by amount and rounded half up, which is
    /// deterministic, stays inside the same finite set of values, and is the
    /// only honest thing to do with "three hundred units of ambient water met
    /// a hundred units of hot water".
    pub fn blend(a: Stuff, na: u64, b: Stuff, nb: u64) -> Stuff {
        if na == 0 {
            return b;
        }
        if nb == 0 || a == b {
            return a;
        }
        let mix = |x: u8, y: u8| -> u8 {
            let n = na + nb;
            (((x as u64 * na + y as u64 * nb) * 2 + n) / (2 * n)) as u8
        };
        Stuff {
            subst: a.subst,
            q: Qual {
                temp: mix(a.q.temp, b.q.temp),
                size: mix(a.q.size, b.q.size),
                purity: mix(a.q.purity, b.q.purity),
                // Form does not average -- half a gear is not a thing. The
                // larger share keeps its shape, and a tie goes to what was
                // already in the buffer.
                form: if nb > na { b.q.form } else { a.q.form },
                speed: mix(a.q.speed, b.q.speed),
            },
        }
    }

    /// Whether these may share a buffer. Same substance blends; anything else
    /// is contamination and is refused at the port.
    pub fn mixes_with(&self, other: &Stuff) -> bool {
        self.subst == other.subst
    }

    /// The name the outer game would use.
    pub fn name(&self) -> &'static str {
        self.subst.title()
    }

    /// The properties worth saying out loud, for this substance, right now.
    pub fn note(&self) -> String {
        let mut bits: Vec<String> = Vec::new();
        match self.domain() {
            Domain::Material => {
                bits.push(SIZE_NAMES[(self.q.size as usize).min(3)].to_string());
                if self.q.form != FORM_RAW {
                    bits.push(FORM_NAMES[(self.q.form as usize).min(4)].to_string());
                }
                bits.push(format!("{}% pure", self.q.purity));
                if self.q.temp > 0 {
                    bits.push(TEMP_NAMES[(self.q.temp as usize).min(9)].to_string());
                }
            }
            Domain::Fluid | Domain::Gas | Domain::Heat => {
                bits.push(TEMP_NAMES[(self.q.temp as usize).min(9)].to_string());
                if self.subst.is_matter() && self.q.purity < 100 {
                    bits.push(format!("{}% pure", self.q.purity));
                }
            }
            Domain::Rotary => bits.push(format!("speed {}", self.q.speed)),
            Domain::Mech | Domain::Electrical => {}
        }
        bits.join(", ")
    }

    /// Name and properties, in one line, for a panel.
    pub fn label(&self) -> String {
        let n = self.note();
        if n.is_empty() {
            self.name().to_string()
        } else {
            format!("{} ({})", self.name(), n)
        }
    }

    /// Six bytes, for the state key an orbit is found by.
    pub fn bytes(&self) -> [u8; 6] {
        [self.subst as u8, self.q.temp, self.q.size, self.q.purity, self.q.form, self.q.speed]
    }

    pub fn to_json(&self) -> crate::json::Json {
        crate::json::Json::obj()
            .set("subst", self.subst.tag())
            .set("name", self.subst.title())
            .set("domain", self.domain().tag())
            .set("temp", self.q.temp as i64)
            .set("size", self.q.size as i64)
            .set("purity", self.q.purity as i64)
            .set("form", self.q.form as i64)
            .set("speed", self.q.speed as i64)
            .set("label", self.label())
            .set("note", self.note())
    }
}

impl fmt::Display for Stuff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label())
    }
}

/// One port's contents: what is in it, and how much.
///
/// Empty means *empty* -- the stuff is reset with the last unit, so that two
/// machines holding nothing hold the same nothing and an orbit can close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Buf {
    pub stuff: Stuff,
    pub qty: u64,
}

impl Buf {
    pub fn empty(subst: Subst) -> Buf {
        Buf { stuff: Stuff::fresh(subst), qty: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.qty == 0
    }

    /// Whether `s` may be poured in here at all.
    pub fn takes(&self, s: &Stuff) -> bool {
        self.qty == 0 || self.stuff.mixes_with(s)
    }

    pub fn put(&mut self, s: Stuff, n: u64) {
        if n == 0 {
            return;
        }
        self.stuff = Stuff::blend(self.stuff, self.qty, s, n);
        self.qty += n;
    }

    /// Take `n`, or everything if there is less. The stuff comes back as it
    /// was; drawing does not change what is left behind.
    pub fn take(&mut self, n: u64) -> (Stuff, u64) {
        let got = n.min(self.qty);
        let s = self.stuff;
        self.qty -= got;
        if self.qty == 0 {
            // Forget what it was. A buffer that remembers the temperature of
            // water it no longer has is a buffer whose state never repeats.
            self.stuff = Stuff::fresh(s.subst);
        }
        (s, got)
    }
}
