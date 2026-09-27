//! The encounter, as data: one route, twelve structures, six batteries, two
//! kinds of attacker and four factory sectors.
//!
//! None of this changes during a fight. What a fight changes -- how much of a
//! wall is left, where a cohort is, which way a turret is facing -- lives in
//! `fight`, indexed by position in these tables, so a checkpoint never has to
//! say what a wall *is*, only how much of it there still is.
//!
//! ```text
//!        Quarry                 Mortar pit          North guns
//!                                                          spawn
//!        (outside)   +------- Smelting -----+            <======
//!                    |       conveyor |     |   Outer wall
//!    <== Works gate == Smelter hall ==#== Inner wall == Redoubt
//!                    |                |     |    South guns
//!        Works       +----------------------+
//!        (outside)          Yard        Hall battery
//! ```

use super::{P, MT, TICK_RATE};
use crate::json::Json;
use crate::model::Tick;

/// The field, in tiles.
pub const W: i64 = 96;
pub const H: i64 = 60;

/// The approach route, east to west. Cohorts walk it leg by leg; a node with a
/// structure standing on it stops them until the structure is gone.
pub const ROUTE: &[P] = &[
    P::tiles(95, 13),
    P::tiles(81, 13),
    P::tiles(70, 23),
    P::tiles(62, 30),
    P::tiles(54, 30),
    P::tiles(47, 30),
    P::tiles(40, 30),
    P::tiles(31, 30),
    P::tiles(22, 30),
];

/// Route length of each leg, in milli-tiles.
pub fn leg_len(leg: usize) -> i64 {
    super::dist(ROUTE[leg], ROUTE[leg + 1])
}

/// Distance along the route to node `n`.
pub fn along(n: usize) -> i64 {
    (0..n).map(leg_len).sum()
}

// ================================================================ attackers

#[derive(Clone, Copy, Debug)]
pub struct KindDef {
    pub name: &'static str,
    pub hp: u16,
    /// Milli-tiles a tick.
    pub speed: i64,
    /// Milli-hp a tick, per member, against whatever is in the way.
    pub bite: i64,
}

pub const KINDS: &[KindDef] = &[
    KindDef { name: "runner", hp: 4, speed: 55, bite: 6 },
    KindDef { name: "brute", hp: 12, speed: 26, bite: 20 },
];

/// How many packets a wave is released as. The number of cohorts a wave starts
/// with, whatever its size.
pub const PACKETS: usize = 12;
/// Ticks between packets, before jitter.
pub const PACKET_GAP: Tick = 75;

// ================================================================ structures

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SKind {
    Wall,
    /// A gun pit: a battery stands in it, and stops firing when it falls.
    Pit,
    Belt,
    Hall,
    Gate,
    Shed,
}

impl SKind {
    pub fn word(self) -> &'static str {
        match self {
            SKind::Wall => "wall",
            SKind::Pit => "pit",
            SKind::Belt => "belt",
            SKind::Hall => "hall",
            SKind::Gate => "gate",
            SKind::Shed => "shed",
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
    /// The route node it stands on, if the route goes through it.
    pub node: Option<usize>,
    pub tie: Option<Tie>,
}

pub const SMELTING: usize = 1;
pub const YARD: usize = 2;

pub const STRUCTURES: &[StructDef] = &[
    StructDef { name: "Outer wall", kind: SKind::Wall, at: ROUTE[2], w: 2, h: 9, hp: 3_200, node: Some(2), tie: None },
    StructDef { name: "Redoubt", kind: SKind::Pit, at: ROUTE[3], w: 4, h: 4, hp: 2_000, node: Some(3), tie: None },
    StructDef { name: "Inner wall", kind: SKind::Wall, at: ROUTE[4], w: 2, h: 11, hp: 4_200, node: Some(4), tie: None },
    StructDef {
        name: "Ore conveyor",
        kind: SKind::Belt,
        at: ROUTE[5],
        w: 1,
        h: 18,
        hp: 420,
        node: Some(5),
        tie: Some(Tie { sector: SMELTING, node: "Conveyor" }),
    },
    StructDef {
        name: "Smelter hall",
        kind: SKind::Hall,
        at: ROUTE[6],
        w: 7,
        h: 6,
        hp: 6_000,
        node: Some(6),
        tie: Some(Tie { sector: SMELTING, node: "Smelters" }),
    },
    StructDef { name: "Works gate", kind: SKind::Gate, at: ROUTE[7], w: 2, h: 7, hp: 3_000, node: Some(7), tie: None },
    StructDef { name: "North pit", kind: SKind::Pit, at: P::tiles(73, 5), w: 4, h: 4, hp: 2_000, node: None, tie: None },
    StructDef { name: "South pit", kind: SKind::Pit, at: P::tiles(62, 42), w: 4, h: 4, hp: 2_000, node: None, tie: None },
    StructDef { name: "Hall pit", kind: SKind::Pit, at: P::tiles(45, 41), w: 3, h: 3, hp: 1_600, node: None, tie: None },
    StructDef { name: "Mortar pit", kind: SKind::Pit, at: P::tiles(40, 13), w: 4, h: 4, hp: 2_000, node: None, tie: None },
    StructDef { name: "Last ditch", kind: SKind::Pit, at: P::tiles(34, 24), w: 3, h: 3, hp: 1_600, node: None, tie: None },
    StructDef {
        name: "Yard sheds",
        kind: SKind::Shed,
        at: P::tiles(40, 50),
        w: 8,
        h: 4,
        hp: 3_000,
        node: None,
        tie: Some(Tie { sector: YARD, node: "Presses" }),
    },
];

/// Where the conveyor physically runs, for drawing: it crosses the route.
pub const BELT_RUN: (P, P) = (P::tiles(47, 21), P::tiles(47, 39));

// ================================================================ batteries

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
    pub reload: Tick,
    /// Milli-tiles a tick.
    pub shell_speed: i64,
    /// Shortest flight, in ticks: a lobbed shell takes a while even at
    /// point-blank range.
    pub min_flight: Tick,
    pub damage: u16,
    /// Attackers one shell can hurt, however many are standing there.
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
    /// Turrets in the battery. One population: they lay together and fire one
    /// volley together, so a volley is one projectile record with a count.
    pub count: u32,
    /// Index into `STRUCTURES`: the pit it stands in.
    pub pit: usize,
    /// Milli-tiles.
    pub range: i64,
    /// Where it faces before it has ever fired.
    pub rest: i32,
}

pub const BATTERIES: &[BatteryDef] = &[
    BatteryDef { name: "North guns", gun: Gun::Cannon, count: 3, pit: 6, range: 21 * MT, rest: 16_384 },
    BatteryDef { name: "Redoubt", gun: Gun::Howitzer, count: 2, pit: 1, range: 27 * MT, rest: 0 },
    BatteryDef { name: "South guns", gun: Gun::Cannon, count: 4, pit: 7, range: 20 * MT, rest: 49_152 },
    BatteryDef { name: "Hall battery", gun: Gun::Cannon, count: 2, pit: 8, range: 16 * MT, rest: 49_152 },
    BatteryDef { name: "Mortars", gun: Gun::Howitzer, count: 3, pit: 9, range: 38 * MT, rest: 8_000 },
    BatteryDef { name: "Last ditch", gun: Gun::Cannon, count: 3, pit: 10, range: 13 * MT, rest: 0 },
];

// ================================================================== sectors

/// One part of the factory: an ordinary plant, in the solver's own language.
#[derive(Clone, Copy, Debug)]
pub struct SectorSpec {
    pub name: &'static str,
    /// Tiles: x0, y0, x1, y1.
    pub rect: (i64, i64, i64, i64),
    /// The item this sector exists to make, for the one number a view shows.
    pub product: &'static str,
    pub src: &'static str,
}

pub const SECTORS: &[SectorSpec] = &[
    SectorSpec {
        name: "Quarry",
        rect: (2, 3, 18, 21),
        product: "Gravel",
        src: "item Ore
item Gravel

blueprint Quarry {
    source  Diggers x4000   { produces 20 Ore every 120 ticks }
    storage Face            { capacity 400000  policy round_robin }
    process Crushers x1600  { consumes 40 Ore takes 240 ticks produces 30 Gravel }
    storage Heap            { capacity 400000  policy round_robin }
    sink    Haulage x200    { consumes 90 Gravel every 60 ticks }

    wire Diggers -> Face -> Crushers -> Heap -> Haulage
}

deploy 1 x Quarry
",
    },
    SectorSpec {
        name: "Smelting",
        rect: (28, 18, 51, 40),
        product: "Iron",
        src: "item Ore
item Iron

blueprint Smelting {
    source  Pit x3000       { produces 20 Ore every 60 ticks }
    storage OreYard         { capacity 600000  policy round_robin }
    link    Conveyor x240   { moves 50 Ore takes 60 ticks returns 60 ticks }
    storage Hopper          { capacity 200000  policy round_robin }
    process Smelters x6000  { consumes 10 Ore takes 600 ticks produces 5 Iron }
    storage IronBay         { capacity 300000  policy round_robin }
    sink    Dispatch x300   { consumes 50 Iron every 300 ticks }

    wire Pit -> OreYard -> Conveyor -> Hopper -> Smelters -> IronBay -> Dispatch
}

deploy 1 x Smelting
",
    },
    SectorSpec {
        name: "Yard",
        rect: (30, 44, 51, 57),
        product: "Gear",
        src: "item Iron
item Gear

blueprint Yard {
    source  Scrap x800      { produces 10 Iron every 30 ticks }
    storage Stock           { capacity 200000  policy round_robin }
    process Presses x1200   { consumes 12 Iron takes 180 ticks produces 6 Gear }
    storage Crates          { capacity 200000  policy round_robin }
    sink    Loading x60     { consumes 60 Gear every 90 ticks }

    wire Scrap -> Stock -> Presses -> Crates -> Loading
}

deploy 1 x Yard
",
    },
    SectorSpec {
        name: "Works",
        rect: (2, 40, 18, 57),
        product: "Frame",
        src: "item Plate
item Frame

blueprint Works {
    source  Mill x600       { produces 30 Plate every 90 ticks }
    storage Plates          { capacity 300000  policy round_robin }
    process Welders x2400   { consumes 8 Plate takes 360 ticks produces 2 Frame }
    storage Frames          { capacity 100000  policy round_robin }
    sink    Rail x40        { consumes 40 Frame every 120 ticks }

    wire Mill -> Plates -> Welders -> Frames -> Rail
}

deploy 1 x Works
",
    },
];

// =================================================================== domain

/// A rectangle in milli-tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x0: i64,
    pub y0: i64,
    pub x1: i64,
    pub y1: i64,
}

impl Rect {
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.y0 < o.y1 && o.y0 < self.y1
    }
    pub fn of_tiles(r: (i64, i64, i64, i64)) -> Rect {
        Rect { x0: r.0 * MT, y0: r.1 * MT, x1: r.2 * MT, y1: r.3 * MT }
    }
    pub fn to_json(&self) -> Json {
        Json::obj().set("x0", self.x0).set("y0", self.y0).set("x1", self.x1).set("y1", self.y1)
    }
}

/// How far past the things that can be hit the domain extends.
const PAD: i64 = 3 * MT;

/// The combat domain: everything a fight can touch.
///
/// Derived, not drawn -- the bounding box of the route, every structure and
/// every battery, padded. The route ends where the fight stops being a fight:
/// a cohort that walks off the last node has left the domain and the factory
/// counts it as a raid, not as something to keep simulating.
pub fn domain() -> Rect {
    let mut r = Rect { x0: i64::MAX, y0: i64::MAX, x1: i64::MIN, y1: i64::MIN };
    let mut take = |p: P, hw: i64, hh: i64| {
        r.x0 = r.x0.min(p.x - hw);
        r.y0 = r.y0.min(p.y - hh);
        r.x1 = r.x1.max(p.x + hw);
        r.y1 = r.y1.max(p.y + hh);
    };
    for &p in ROUTE {
        take(p, 0, 0);
    }
    for s in STRUCTURES {
        take(s.at, s.w * MT / 2, s.h * MT / 2);
    }
    Rect {
        x0: (r.x0 - PAD).max(0),
        y0: (r.y0 - PAD).max(0),
        x1: (r.x1 + PAD).min(W * MT),
        y1: (r.y1 + PAD).min(H * MT),
    }
}

/// The whole layout, for a view. Never changes, so a client fetches it once.
pub fn to_json() -> Json {
    let p = |p: P| Json::arr([p.x, p.y]);
    Json::obj()
        .set("ok", true)
        .set("w", W)
        .set("h", H)
        .set("mt", MT)
        .set("tickRate", TICK_RATE)
        .set("route", Json::Arr(ROUTE.iter().map(|&n| p(n)).collect()))
        .set("belt", Json::arr([p(BELT_RUN.0), p(BELT_RUN.1)]))
        .set("domain", domain().to_json())
        .set(
            "kinds",
            Json::Arr(
                KINDS
                    .iter()
                    .map(|k| {
                        Json::obj()
                            .set("name", k.name)
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
                            .set("node", s.node.map(|n| n as u64))
                            .set(
                                "tie",
                                s.tie.map(|t| format!("{} / {}", SECTORS[t.sector].name, t.node)),
                            )
                    })
                    .collect(),
            ),
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
                    .map(|s| {
                        Json::obj()
                            .set("name", s.name)
                            .set("rect", Rect::of_tiles(s.rect).to_json())
                            .set("product", s.product)
                            .set("inside", Rect::of_tiles(s.rect).overlaps(&domain()))
                    })
                    .collect(),
            ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_sectors_inside_two_outside() {
        let d = domain();
        let inside: Vec<&str> = SECTORS
            .iter()
            .filter(|s| Rect::of_tiles(s.rect).overlaps(&d))
            .map(|s| s.name)
            .collect();
        assert_eq!(inside, ["Smelting", "Yard"]);
    }

    #[test]
    fn every_structure_on_the_route_is_on_its_node() {
        for s in STRUCTURES {
            if let Some(n) = s.node {
                assert_eq!(s.at, ROUTE[n], "{}", s.name);
            }
        }
    }
}
