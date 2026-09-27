//! The ground a disturbance happens on, as data: experiment 15's plot in 2037,
//! the interface on the Rift, what comes through it, where that material ends
//! up, the roads between, and the things a player can switch on.
//!
//! None of this changes during a disturbance. What does -- how much strain a
//! site is carrying, whether it has torn, where a cohort is -- lives in `fight`,
//! indexed by position in these tables.
//!
//! ```text
//!    Pylon Line ---------------------------------------------------------
//!                                        Oakshaw (scrub)   N.anchor  RIFT
//!    Kestrel Works                                                  [gantry]
//!    (the foundry)                                            Drove guns |
//!    ==== The Drove (the A-road) ===========================+============+
//!                            Street        Tarrant Street    |  S.anchor |
//!                            mortars       [lathe shop]      |    [ore yard]
//!    ---- The Cut ----------------------------------------------+----------
//!                                                   Cut guns  [crusher house]
//!    ~~~~ Mill Race ~~~~~~~~~~~~~~~~~~~~~~~~~
//! ```
//!
//! # Two lanes, two centuries, one door
//!
//! Both fractures land on the Rift, which in 2037 has a gantry on it. The deep
//! lane brings 1890 ore forward a hundred and forty-seven years into the ore
//! yard; the near lane brings 2070 lathes back thirty-three years into the
//! lathe shop. Every tonne that crosses leaves `years` of strain behind it:
//! some on the gantry it came through and the rest wherever it is put down.

use super::{P, MT, TICK_RATE};
use crate::json::Json;
use crate::slice::land::{self, LAND};
use crate::slice::Phase;

/// The part of the plot the view shows, in tiles. The plot is experiment 15's
/// seventy-two tiles square; below the Mill Race there is nothing to look at.
pub const W: i64 = land::PLOT as i64;
pub const H: i64 = 46;

/// The phase the district stands in, and the two it is importing from.
pub const HERE: Phase = Phase::P2037;

/// A strain component is one of these: material from 1890, or from 2070.
pub const ORIGINS: [Phase; 2] = [Phase::P1890, Phase::P2070];

// ==================================================================== roads

/// Where manifestations can walk: the Drove, the Cut, and the yards between.
pub const NODES: &[(&str, P)] = &[
    ("Gantry", P::tiles(65, 11)),
    ("Drove east", P::tiles(64, 17)),
    ("Drove", P::tiles(55, 17)),
    ("Drove west", P::tiles(44, 17)),
    ("Drove far west", P::tiles(28, 17)),
    ("Kestrel gate", P::tiles(15, 17)),
    ("Ore yard", P::tiles(60, 23)),
    ("Lathe shop", P::tiles(46, 22)),
    ("Cut east", P::tiles(62, 28)),
    ("Cut", P::tiles(50, 28)),
    ("Crusher house", P::tiles(59, 33)),
    ("Cut west", P::tiles(34, 28)),
];

pub const EDGES: &[(usize, usize)] = &[
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 4),
    (4, 5),
    (1, 6),
    (6, 8),
    (8, 9),
    (9, 11),
    (11, 4),
    (2, 7),
    (3, 7),
    (7, 9),
    (8, 10),
];

pub fn node(n: usize) -> P {
    NODES[n].1
}

pub fn edge_len(a: usize, b: usize) -> i64 {
    super::dist(node(a), node(b))
}

/// Shortest paths over the road graph: `next[a][b]` is the node to walk to from
/// `a` on the way to `b`, and `far[a][b]` how far it is. Floyd-Warshall over a
/// dozen nodes, in integers, so it is the same table everywhere.
pub struct Roads {
    pub next: Vec<Vec<usize>>,
    pub far: Vec<Vec<i64>>,
}

pub fn roads() -> &'static Roads {
    static R: std::sync::OnceLock<Roads> = std::sync::OnceLock::new();
    R.get_or_init(|| {
        let n = NODES.len();
        let inf = i64::MAX / 4;
        let mut far = vec![vec![inf; n]; n];
        let mut next = vec![vec![usize::MAX; n]; n];
        for i in 0..n {
            far[i][i] = 0;
            next[i][i] = i;
        }
        for &(a, b) in EDGES {
            let d = edge_len(a, b);
            far[a][b] = d;
            far[b][a] = d;
            next[a][b] = b;
            next[b][a] = a;
        }
        for k in 0..n {
            for i in 0..n {
                for j in 0..n {
                    let via = far[i][k] + far[k][j];
                    if via < far[i][j] {
                        far[i][j] = via;
                        next[i][j] = next[i][k];
                    }
                }
            }
        }
        Roads { next, far }
    })
}

// ============================================================ manifestations

#[derive(Clone, Copy, Debug)]
pub struct KindDef {
    pub name: &'static str,
    /// Which strain component this is made of, and so which century's
    /// material it goes looking for.
    pub origin: usize,
    pub hp: u16,
    /// Milli-tiles a tick.
    pub speed: i64,
    /// Milli-hp a tick, per member, against the building it is standing in.
    pub bite: i64,
    /// Milli-strain a tick, per member: the anachronism it takes back.
    pub absorb: i64,
}

/// Kind `i` is made of strain component `i`.
pub const KINDS: &[KindDef] = &[
    // Slow, many, soft: a crowd of something that belongs in a mining valley.
    KindDef { name: "echo", origin: 0, hp: 6, speed: 22, bite: 2, absorb: 60 },
    // Quick, few, hard: something that has not been built yet.
    KindDef { name: "glint", origin: 1, hp: 10, speed: 60, bite: 6, absorb: 60 },
];

// ================================================================= structures

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SKind {
    Gantry,
    Yard,
    Shop,
    Crusher,
    Works,
    Anchor,
    /// A gun pit: a battery stands in it, and stops firing when it falls.
    Pit,
}

impl SKind {
    pub fn word(self) -> &'static str {
        match self {
            SKind::Gantry => "gantry",
            SKind::Yard => "yard",
            SKind::Shop => "shop",
            SKind::Crusher => "crusher",
            SKind::Works => "works",
            SKind::Anchor => "anchor",
            SKind::Pit => "pit",
        }
    }
}

/// What a structure *is* to the factory: a node of one sector's plant.
#[derive(Clone, Copy, Debug)]
pub struct Tie {
    pub sector: usize,
    pub node: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct StructDef {
    pub name: &'static str,
    pub kind: SKind,
    pub at: P,
    /// Drawn size, in tiles.
    pub w: i64,
    pub h: i64,
    /// Whole hit points. The fight counts milli-hp.
    pub hp: i64,
    pub tie: Option<Tie>,
}

pub const INTERFACE: usize = 0;
pub const CRUSHING: usize = 1;
pub const SHOP: usize = 2;
pub const FOUNDRY: usize = 3;

pub const GANTRY: usize = 0;
pub const ORE_YARD: usize = 1;
pub const LATHE_SHOP: usize = 2;
pub const CRUSHER: usize = 3;

pub const STRUCTURES: &[StructDef] = &[
    StructDef {
        name: "Gantry",
        kind: SKind::Gantry,
        at: P::tiles(65, 11),
        w: 6,
        h: 6,
        hp: 6_000,
        tie: Some(Tie { sector: INTERFACE, node: "Haul" }),
    },
    StructDef {
        name: "Ore yard",
        kind: SKind::Yard,
        at: P::tiles(60, 23),
        w: 8,
        h: 5,
        hp: 8_000,
        tie: Some(Tie { sector: INTERFACE, node: "Sorters" }),
    },
    StructDef {
        name: "Lathe shop",
        kind: SKind::Shop,
        at: P::tiles(46, 22),
        w: 7,
        h: 5,
        hp: 5_000,
        tie: Some(Tie { sector: SHOP, node: "Lathes" }),
    },
    StructDef {
        name: "Crusher house",
        kind: SKind::Crusher,
        at: P::tiles(59, 33),
        w: 6,
        h: 4,
        hp: 4_000,
        tie: Some(Tie { sector: CRUSHING, node: "Crushers" }),
    },
    StructDef { name: "Kestrel Foundry", kind: SKind::Works, at: P::tiles(15, 10), w: 12, h: 7, hp: 9_000, tie: None },
    StructDef { name: "North anchor", kind: SKind::Anchor, at: P::tiles(58, 6), w: 2, h: 2, hp: 2_500, tie: None },
    StructDef { name: "South anchor", kind: SKind::Anchor, at: P::tiles(53, 21), w: 2, h: 2, hp: 2_500, tie: None },
    StructDef { name: "Drove guns", kind: SKind::Pit, at: P::tiles(59, 14), w: 3, h: 3, hp: 2_000, tie: None },
    StructDef { name: "Cut guns", kind: SKind::Pit, at: P::tiles(54, 28), w: 3, h: 3, hp: 2_000, tie: None },
    StructDef { name: "Street mortars", kind: SKind::Pit, at: P::tiles(36, 22), w: 4, h: 4, hp: 2_000, tie: None },
];

// ====================================================================== sites

/// A place strain collects, and where the world tears when there is too much.
#[derive(Clone, Copy, Debug)]
pub struct SiteDef {
    pub name: &'static str,
    /// The road node it stands on: where a rupture releases from and where a
    /// manifestation has to get to.
    pub node: usize,
    pub structure: usize,
    /// Which strain component it *holds* as stock, if any. The gantry holds
    /// nothing: material passes through it. A manifestation only goes looking
    /// for stock.
    pub holds: Option<usize>,
    /// Milli-strain at which it tears.
    pub open: i64,
}

/// A tonne-year of strain, in the milli-units the fight counts.
pub const TY: i64 = 1000;

pub const SITES: &[SiteDef] = &[
    // The light is already wrong here; it does not take much.
    SiteDef { name: "Gantry", node: 0, structure: GANTRY, holds: None, open: 40_000 * TY },
    SiteDef { name: "Ore yard", node: 6, structure: ORE_YARD, holds: Some(0), open: 120_000 * TY },
    SiteDef { name: "Lathe shop", node: 7, structure: LATHE_SHOP, holds: Some(1), open: 40_000 * TY },
];

/// A rupture seals when strain falls to this fraction of what tore it, in
/// thousandths. Hysteresis, so it does not flicker at the threshold.
pub const CLOSE_PM: i64 = 250;

pub fn close_at(site: usize) -> i64 {
    SITES[site].open * CLOSE_PM / 1000
}

/// Bleed bands: strain at or above `open x BLEED_X[b]` bleeds to `BLEED_R[b]`.
pub const BLEED_X: [i64; 3] = [0, 2, 4];
pub const BLEED_R: [i64; 3] = [7 * MT, 10 * MT, 14 * MT];

/// How far a rupture can ever reach. The sectors that wake when it opens are
/// the ones this circle touches.
pub fn reach(site: usize) -> (P, i64) {
    (node(SITES[site].node), BLEED_R[BLEED_X.len() - 1])
}

/// A rupture vents every `EMIT_EVERY` ticks, releasing `VENT_PM` thousandths of
/// what it is carrying as manifestations of `COST` milli-strain a member.
pub const EMIT_EVERY: u64 = TICK_RATE;
pub const VENT_PM: i64 = 300;
pub const COST: i64 = 100 * TY;

// ====================================================================== lanes

#[derive(Clone, Copy, Debug)]
pub struct LaneDef {
    pub tag: &'static str,
    pub name: &'static str,
    /// Strain component, and so the century it comes out of.
    pub origin: usize,
    pub item: &'static str,
    /// Where it is put down.
    pub dest: usize,
    /// What it looks like on the ground, for drawing the stream.
    pub path: &'static [P],
    /// And where it goes instead when it is laundered.
    pub launder_path: Option<&'static [P]>,
}

impl LaneDef {
    pub fn years(&self) -> u32 {
        ORIGINS[self.origin].displacement(HERE)
    }
    /// Megawatts to hold its fracture open: experiment 15's formula.
    pub fn mw(&self) -> u64 {
        crate::slice::gate::draw(self.years())
    }
}

pub const LANES: &[LaneDef] = &[
    LaneDef {
        tag: "deep",
        name: "Deep fracture",
        origin: 0,
        item: "1890 ore",
        dest: 1,
        path: &[P::tiles(65, 11), P::tiles(64, 17), P::tiles(60, 23)],
        launder_path: Some(&[P::tiles(65, 11), P::tiles(66, 17), P::tiles(66, 28), P::tiles(59, 33)]),
    },
    LaneDef {
        tag: "near",
        name: "Near corridor",
        origin: 1,
        item: "2070 lathes",
        dest: 2,
        path: &[P::tiles(65, 11), P::tiles(64, 17), P::tiles(46, 17), P::tiles(46, 22)],
        launder_path: None,
    },
];

/// Thousandths of each tonne's strain left on the gantry it came through; the
/// rest goes where it is put down.
pub const GANTRY_SHARE: i64 = 300;

// ==================================================================== anchors

#[derive(Clone, Copy, Debug)]
pub struct AnchorDef {
    pub name: &'static str,
    pub structure: usize,
    /// Milli-tiles. A site inside it is pinned to 2037 and drained.
    pub range: i64,
}

pub const ANCHORS: &[AnchorDef] = &[
    AnchorDef { name: "North anchor", structure: 5, range: 9 * MT },
    AnchorDef { name: "South anchor", structure: 6, range: 8 * MT },
];

/// Milli-strain a tick an anchor takes out of each component at each site it
/// covers.
pub const ANCHOR_DRAIN: i64 = 20 * TY;

// ======================================================================= grid

/// What the district's grid can deliver. Everything that holds time in place
/// is paid for out of it, so an anchor is paid for in fracture.
pub const GRID_MW: u64 = 180;
pub const ANCHOR_MW: u64 = 25;
pub const LAUNDER_MW: u64 = 20;

// ================================================================== batteries

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gun {
    /// Fast, flat, light. Tracers.
    Cannon,
    /// Slow, lobbed, heavy. Shells.
    Howitzer,
}

#[derive(Clone, Copy, Debug)]
pub struct GunDef {
    pub name: &'static str,
    /// Binary angle units a tick.
    pub turn: i32,
    pub reload: u64,
    /// Milli-tiles a tick.
    pub shell_speed: i64,
    /// Shortest flight, in ticks.
    pub min_flight: u64,
    pub damage: u16,
    /// Members one shell can hurt, however many are standing there.
    pub hits: u64,
    /// Milli-tiles.
    pub blast: i64,
}

pub fn gun(g: Gun) -> &'static GunDef {
    match g {
        Gun::Cannon => &GunDef {
            name: "autocannon",
            turn: 520,
            reload: 24,
            shell_speed: 1_300,
            min_flight: 2,
            damage: 2,
            hits: 5,
            blast: 1_300,
        },
        Gun::Howitzer => &GunDef {
            name: "howitzer",
            turn: 140,
            reload: 150,
            shell_speed: 380,
            min_flight: 40,
            damage: 8,
            hits: 40,
            blast: 3_400,
        },
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BatteryDef {
    pub name: &'static str,
    pub gun: Gun,
    pub count: u32,
    /// Index into `STRUCTURES`: the pit it stands in.
    pub pit: usize,
    pub range: i64,
    pub rest: i32,
}

pub const BATTERIES: &[BatteryDef] = &[
    BatteryDef { name: "Drove guns", gun: Gun::Cannon, count: 3, pit: 7, range: 13 * MT, rest: 0 },
    BatteryDef { name: "Cut guns", gun: Gun::Cannon, count: 4, pit: 8, range: 15 * MT, rest: 49_152 },
    BatteryDef { name: "Street mortars", gun: Gun::Howitzer, count: 2, pit: 9, range: 28 * MT, rest: 0 },
];

// ==================================================================== sectors

/// One part of the factory: an ordinary plant, in the solver's own language.
#[derive(Clone, Copy, Debug)]
pub struct SectorSpec {
    pub name: &'static str,
    /// Tiles: x0, y0, x1, y1.
    pub rect: (i64, i64, i64, i64),
    pub product: &'static str,
    pub src: &'static str,
}

pub const SECTORS: &[SectorSpec] = &[
    SectorSpec {
        name: "Interface",
        rect: (54, 4, 70, 27),
        product: "Sized",
        src: "item Ore
item Sized

blueprint Interface {
    source  Crossing x3000  { produces 20 Ore every 60 ticks }
    storage Apron           { capacity 600000  policy round_robin }
    link    Haul x240       { moves 50 Ore takes 60 ticks returns 60 ticks }
    storage OreYard         { capacity 200000  policy round_robin }
    process Sorters x6000   { consumes 10 Ore takes 600 ticks produces 5 Sized }
    storage SizedBay        { capacity 300000  policy round_robin }
    sink    Rail x300       { consumes 50 Sized every 300 ticks }

    wire Crossing -> Apron -> Haul -> OreYard -> Sorters -> SizedBay -> Rail
}

deploy 1 x Interface
",
    },
    SectorSpec {
        name: "Crushing",
        rect: (50, 29, 68, 40),
        product: "Concentrate",
        src: "item Ore
item Concentrate

blueprint Crushing {
    source  Receipt x800    { produces 10 Ore every 30 ticks }
    storage Bins            { capacity 200000  policy round_robin }
    process Crushers x1200  { consumes 12 Ore takes 180 ticks produces 6 Concentrate }
    storage ConcBay         { capacity 200000  policy round_robin }
    sink    Loadout x60     { consumes 60 Concentrate every 90 ticks }

    wire Receipt -> Bins -> Crushers -> ConcBay -> Loadout
}

deploy 1 x Crushing
",
    },
    SectorSpec {
        name: "Machine shop",
        rect: (38, 18, 53, 27),
        product: "Gear",
        src: "item Billet
item Gear

blueprint Shop {
    source  Stock x600      { produces 30 Billet every 90 ticks }
    storage Racks           { capacity 300000  policy round_robin }
    process Lathes x2400    { consumes 8 Billet takes 360 ticks produces 2 Gear }
    storage GearBay         { capacity 100000  policy round_robin }
    sink    Dispatch x40    { consumes 40 Gear every 120 ticks }

    wire Stock -> Racks -> Lathes -> GearBay -> Dispatch
}

deploy 1 x Shop
",
    },
    SectorSpec {
        name: "Foundry",
        rect: (4, 4, 26, 16),
        product: "Iron",
        src: "item Concentrate
item Iron

blueprint Foundry {
    source  Charging x4000  { produces 20 Concentrate every 120 ticks }
    storage Burden          { capacity 400000  policy round_robin }
    process Furnaces x1600  { consumes 40 Concentrate takes 240 ticks produces 30 Iron }
    storage Pig             { capacity 400000  policy round_robin }
    sink    Casting x200    { consumes 90 Iron every 60 ticks }

    wire Charging -> Burden -> Furnaces -> Pig -> Casting
}

deploy 1 x Foundry
",
    },
];

// ===================================================================== shapes

/// A rectangle in milli-tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x0: i64,
    pub y0: i64,
    pub x1: i64,
    pub y1: i64,
}

impl Rect {
    pub fn of_tiles(r: (i64, i64, i64, i64)) -> Rect {
        Rect { x0: r.0 * MT, y0: r.1 * MT, x1: r.2 * MT, y1: r.3 * MT }
    }
    /// Whether a circle touches this rectangle.
    pub fn meets_circle(&self, c: P, r: i64) -> bool {
        let x = c.x.clamp(self.x0, self.x1);
        let y = c.y.clamp(self.y0, self.y1);
        super::dist(c, P { x, y }) <= r
    }
    pub fn to_json(&self) -> Json {
        Json::obj().set("x0", self.x0).set("y0", self.y0).set("x1", self.x1).set("y1", self.y1)
    }
}

/// Whether a rupture at `site` can ever reach sector `i`.
pub fn reaches(site: usize, i: usize) -> bool {
    let (c, r) = reach(site);
    Rect::of_tiles(SECTORS[i].rect).meets_circle(c, r)
}

// ======================================================================= json

/// The whole layout, for a view. Never changes, so a client fetches it once.
pub fn to_json() -> Json {
    let p = |p: P| Json::arr([p.x, p.y]);
    let path = |ps: &[P]| Json::Arr(ps.iter().map(|&q| p(q)).collect());
    let r = roads();
    Json::obj()
        .set("ok", true)
        .set("w", W)
        .set("h", H)
        .set("mt", MT)
        .set("tickRate", TICK_RATE)
        .set("phase", HERE.tag())
        .set("origins", Json::arr(ORIGINS.iter().map(|o| o.tag())))
        .set("nodes", Json::Arr(NODES.iter().map(|(n, at)| Json::obj().set("name", *n).set("at", p(*at))).collect()))
        .set("edges", Json::Arr(EDGES.iter().map(|&(a, b)| Json::arr([a as u64, b as u64])).collect()))
        .set(
            "next",
            Json::Arr(r.next.iter().map(|row| Json::arr(row.iter().map(|&n| n as u64))).collect()),
        )
        .set(
            "kinds",
            Json::Arr(
                KINDS
                    .iter()
                    .map(|k| {
                        Json::obj()
                            .set("name", k.name)
                            .set("origin", ORIGINS[k.origin].tag())
                            .set("hp", k.hp as u64)
                            .set("speed", k.speed)
                    })
                    .collect(),
            ),
        )
        .set(
            "structures",
            Json::Arr(
                STRUCTURES
                    .iter()
                    .map(|s| {
                        Json::obj()
                            .set("name", s.name)
                            .set("kind", s.kind.word())
                            .set("at", p(s.at))
                            .set("w", s.w)
                            .set("h", s.h)
                            .set("hp", s.hp * 1000)
                            .set("tie", s.tie.map(|t| format!("{} / {}", SECTORS[t.sector].name, t.node)))
                    })
                    .collect(),
            ),
        )
        .set(
            "sites",
            Json::Arr(
                SITES
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        Json::obj()
                            .set("name", s.name)
                            .set("node", s.node as u64)
                            .set("at", p(node(s.node)))
                            .set("structure", s.structure as u64)
                            .set("holds", s.holds.map(|h| ORIGINS[h].tag()))
                            .set("open", s.open)
                            .set("close", close_at(i))
                    })
                    .collect(),
            ),
        )
        .set("bleedX", Json::arr(BLEED_X))
        .set("bleedR", Json::arr(BLEED_R))
        .set(
            "lanes",
            Json::Arr(
                LANES
                    .iter()
                    .map(|l| {
                        Json::obj()
                            .set("tag", l.tag)
                            .set("name", l.name)
                            .set("origin", ORIGINS[l.origin].tag())
                            .set("item", l.item)
                            .set("years", l.years() as u64)
                            .set("mw", l.mw())
                            .set("dest", SITES[l.dest].name)
                            .set("path", path(l.path))
                            .set("launderPath", l.launder_path.map(path))
                    })
                    .collect(),
            ),
        )
        .set(
            "anchors",
            Json::Arr(
                ANCHORS
                    .iter()
                    .map(|a| {
                        Json::obj()
                            .set("name", a.name)
                            .set("at", p(STRUCTURES[a.structure].at))
                            .set("structure", a.structure as u64)
                            .set("range", a.range)
                            .set("mw", ANCHOR_MW)
                    })
                    .collect(),
            ),
        )
        .set(
            "grid",
            Json::obj().set("mw", GRID_MW).set("anchorMw", ANCHOR_MW).set("launderMw", LAUNDER_MW),
        )
        .set(
            "batteries",
            Json::Arr(
                BATTERIES
                    .iter()
                    .map(|b| {
                        let g = gun(b.gun);
                        Json::obj()
                            .set("name", b.name)
                            .set("gun", g.name)
                            .set("count", b.count as u64)
                            .set("at", p(STRUCTURES[b.pit].at))
                            .set("pit", b.pit as u64)
                            .set("range", b.range)
                            .set("reload", g.reload)
                            .set("blast", g.blast)
                    })
                    .collect(),
            ),
        )
        .set(
            "sectors",
            Json::Arr(
                SECTORS
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        Json::obj()
                            .set("name", s.name)
                            .set("rect", Rect::of_tiles(s.rect).to_json())
                            .set("product", s.product)
                            .set(
                                "reachedBy",
                                Json::arr(
                                    (0..SITES.len()).filter(|&k| reaches(k, i)).map(|k| SITES[k].name),
                                ),
                            )
                    })
                    .collect(),
            ),
        )
        .set("land", Json::Arr(LAND.iter().map(|f| f.to_json()).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_node_reaches_every_other() {
        let r = roads();
        for a in 0..NODES.len() {
            for b in 0..NODES.len() {
                assert!(r.next[a][b] != usize::MAX, "{} -> {}", NODES[a].0, NODES[b].0);
            }
        }
        assert_eq!(r.next[0][6], 1, "the gantry reaches the ore yard by the Drove");
    }

    #[test]
    fn the_foundry_is_out_of_every_ruptures_reach() {
        for k in 0..SITES.len() {
            assert!(!reaches(k, FOUNDRY), "{} reaches the foundry", SITES[k].name);
        }
        assert!(reaches(0, INTERFACE));
    }

    #[test]
    fn every_site_stands_on_its_node() {
        for s in SITES {
            assert_eq!(STRUCTURES[s.structure].at, node(s.node), "{}", s.name);
        }
    }

    #[test]
    fn the_lanes_cost_what_experiment_15_says() {
        assert_eq!(LANES[0].years(), 147);
        assert_eq!(LANES[0].mw(), 98);
        assert_eq!(LANES[1].years(), 33);
        assert_eq!(LANES[1].mw(), 53);
    }
}
