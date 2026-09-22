//! One piece of ground, written down once, resolved three times.
//!
//! This is the module that has to make a player say *this is the same place*.
//! Not "a similar map" and not "a reskin": the literal same rectangle, with the
//! literal same features at the literal same coordinates, asked what it looked
//! like in three different years.
//!
//! ```text
//!                 1890              2037              2070
//!   Kestrel Reach ore, 400/s        Kestrel Foundry   distribution sheds
//!   Kestrel Spoil ore, 180/s        spent             spent, under scrub
//!   Blackband     coal, 800/s       coal, 45/s        spent
//!   Oakshaw       oak wood          scrub             scrub
//!   Long Wood     oak wood          Tarrant Street    Tarrant Street
//!   The Drove     cart track        the A-road        the A-road
//!   Mill Race     river, 1,600/s    culverted, 1,200/s   culverted, 500/s
//! ```
//!
//! # Why the features are authored and the maps are derived
//!
//! The obvious implementation is three maps. It is also the implementation that
//! quietly stops being the same place the first time somebody nudges a seam
//! twelve tiles east in one of them and not the others -- and *that is the
//! entire claim this experiment is making*. So there is one list of features,
//! each carrying three faces, and the three maps are a fold over it. It is not
//! possible to move the Kestrel ore body in 1890 without moving the foundry
//! that stands on it in 2037, because they are the same row.
//!
//! The test in the brief reads:
//!
//! ```text
//!   1890:  rich iron deposit
//!   2037:  deposit exhausted, factory standing above it
//! ```
//!
//! and it is two rows here rather than one, on purpose. `kestrel` is the part
//! the foundry was built on -- in 2037 it is *occupied*, and the slice refuses
//! to let anybody build there, in a sentence that says why. `kestrel-spoil` is
//! the part that was left open -- in 2037 it is *empty*, visible, named, and
//! worth eight a second. Together they say the two different things a player
//! needs to understand: somebody took the ore, and somebody built on the hole.
//!
//! # What "visibly" means without a browser
//!
//! Two renderings, from the same fold. [`glyphs`] is one character per tile,
//! which is what `slice map` prints and what a test can assert on; [`rgb`] is
//! the same field as pixels, which `slice map --png` writes through experiment
//! 08's PNG writer. Neither is the simulation's business: this module holds
//! *terrain*, the regions in [`super::region`] turn its resource faces into
//! deposits, and nothing anywhere renders by asking the solver a question.
//!
//! [`super::gate`]'s fractures land at `rift`, which is the one feature that is
//! in all three phases and is not a natural object in any of them.

use super::phase::Phase;
use crate::json::Json;
use crate::model::Qty;

/// The side of the plot, in tiles -- and it is one number for all three
/// regions, because they are one place.
pub const PLOT: i32 = 72;

/// What is left in a body somebody has already been through.
///
/// Not zero. A spent deposit a player can see and get *nothing* out of is a
/// wall with a label on it; one that yields eight a second is an insult with a
/// number attached, which is the correct feeling and is also honest -- the ore
/// is still in the walls, it is simply not worth a factory.
pub const SPENT_YIELD: Qty = 8;

// --------------------------------------------------------------------- faces

/// What stands on a piece of ground in one phase.
///
/// Six layers of one world: what is under it, what grows on it, what crosses
/// it, what was built on it, what supplies it, and the fracture.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face {
    // ---- under it: ground a head can be put on
    Ore(Qty),
    Coal(Qty),
    River(Qty),
    /// A casting table: continuous iron, which is a 2037 thing to have.
    Table(Qty),
    /// A body somebody already emptied. Still there, still named, worth
    /// [`SPENT_YIELD`].
    Spent(&'static str),
    // ---- on it
    Forest,
    Scrub,
    /// A hundred and fifty years of tailings.
    Slag,
    // ---- across it
    Track,
    Road,
    Street,
    // ---- built on it
    Works,
    Sheds,
    Tower,
    Pylons,
    // ---- the fracture
    /// The natural mouth of a fracture: a place where the light is wrong.
    Rift,
    /// The same mouth, with a temporal interface built over it.
    Gantry,
    /// Ground with nothing on it worth drawing.
    Bare,
}

/// Which of the six things a face is, so that "crossing a fracture changes the
/// world" can be *counted* rather than asserted.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Layer {
    Resource,
    Vegetation,
    Route,
    Building,
    Supply,
    Fracture,
    Bare,
}

impl Layer {
    pub fn tag(self) -> &'static str {
        match self {
            Layer::Resource => "resource",
            Layer::Vegetation => "vegetation",
            Layer::Route => "roads",
            Layer::Building => "buildings",
            Layer::Supply => "infrastructure",
            Layer::Fracture => "fracture",
            Layer::Bare => "open ground",
        }
    }
}

impl Face {
    /// One character, for a map that has to fit in a terminal.
    pub fn glyph(self) -> char {
        match self {
            Face::Ore(_) => 'O',
            Face::Coal(_) => 'c',
            Face::River(_) => '~',
            Face::Table(_) => '=',
            Face::Spent(_) => 'x',
            Face::Forest => 'T',
            Face::Scrub => '"',
            Face::Slag => 's',
            Face::Track => '-',
            Face::Road => '#',
            Face::Street => '&',
            Face::Works => 'W',
            Face::Sheds => 'H',
            Face::Tower => 'A',
            Face::Pylons => 'Y',
            Face::Rift => '*',
            Face::Gantry => '@',
            Face::Bare => '.',
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Face::Ore(_) => "iron ore, at the surface",
            Face::Coal(_) => "coal",
            Face::River(_) => "river",
            Face::Table(_) => "casting table",
            Face::Spent(_) => "a body somebody already had",
            Face::Forest => "oak wood",
            Face::Scrub => "scrub",
            Face::Slag => "slag bank",
            Face::Track => "cart track",
            Face::Road => "metalled road",
            Face::Street => "street",
            Face::Works => "standing works",
            Face::Sheds => "sheds",
            Face::Tower => "tower",
            Face::Pylons => "pylons",
            Face::Rift => "an open fracture",
            Face::Gantry => "a temporal interface",
            Face::Bare => "open ground",
        }
    }

    pub fn layer(self) -> Layer {
        match self {
            Face::Ore(_) | Face::Coal(_) | Face::River(_) | Face::Table(_) | Face::Spent(_) => {
                Layer::Resource
            }
            Face::Forest | Face::Scrub | Face::Slag => Layer::Vegetation,
            Face::Track | Face::Road | Face::Street => Layer::Route,
            Face::Works | Face::Sheds | Face::Tower => Layer::Building,
            Face::Pylons => Layer::Supply,
            Face::Rift | Face::Gantry => Layer::Fracture,
            Face::Bare => Layer::Bare,
        }
    }

    /// The deposit this face is, if it is one: the world item and what comes
    /// out of it in a second.
    ///
    /// A spent body is a deposit too. That is the point of it.
    pub fn yields(self) -> Option<(&'static str, Qty)> {
        match self {
            Face::Ore(q) => Some(("IronOre", q)),
            Face::Coal(q) => Some(("Coal", q)),
            Face::River(q) => Some(("Water", q)),
            Face::Table(q) => Some(("IronBillet", q)),
            Face::Spent(item) => Some((item, SPENT_YIELD)),
            _ => None,
        }
    }

    /// Whether there is already something here, so nobody may build on it.
    ///
    /// Buildings, pylons and the fracture mouth. Not roads and not woodland: a
    /// player may put a bay on a road, and clearing trees has never been a
    /// decision in this game.
    pub fn taken(self) -> bool {
        matches!(
            self,
            Face::Works | Face::Sheds | Face::Tower | Face::Pylons | Face::Rift | Face::Gantry
        )
    }

    /// What this face draws as, for the PNG.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Face::Ore(_) => [156, 92, 58],
            Face::Coal(_) => [48, 44, 48],
            Face::River(_) => [58, 108, 148],
            Face::Table(_) => [188, 148, 96],
            Face::Spent(_) => [96, 82, 74],
            Face::Forest => [56, 104, 60],
            Face::Scrub => [122, 132, 86],
            Face::Slag => [86, 74, 68],
            Face::Track => [150, 136, 110],
            Face::Road => [104, 104, 108],
            Face::Street => [132, 130, 134],
            Face::Works => [176, 84, 60],
            Face::Sheds => [148, 148, 156],
            Face::Tower => [200, 196, 188],
            Face::Pylons => [214, 196, 108],
            Face::Rift => [178, 126, 214],
            Face::Gantry => [226, 176, 96],
            Face::Bare => [188, 178, 158],
        }
    }
}

// ------------------------------------------------------------------ features

/// One thing on the ground, and what it is in each of the three phases.
pub struct Feature {
    pub tag: &'static str,
    /// What it is called in 1890, which is usually what it is still called.
    pub name: &'static str,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// 1890, 2037, 2070 -- in that order, indexed by [`Phase::index`].
    pub faces: [Face; 3],
    /// What happened to it, in one sentence, for the panel that explains a
    /// fracture crossing to somebody who has just made one.
    pub became: &'static str,
}

impl Feature {
    pub fn face(&self, p: Phase) -> Face {
        self.faces[p.index()]
    }

    pub fn covers(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    /// Whether this feature's rectangle meets that one.
    pub fn meets(&self, x: i32, y: i32, w: i32, h: i32) -> bool {
        self.x < x + w && x < self.x + self.w && self.y < y + h && y < self.y + self.h
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("tag", self.tag)
            .set("name", self.name)
            .set("x", self.x as i64)
            .set("y", self.y as i64)
            .set("w", self.w as i64)
            .set("h", self.h as i64)
            .set("became", self.became)
            .set(
                "faces",
                Json::Arr(
                    super::phase::PHASES
                        .iter()
                        .map(|p| {
                            let f = self.face(*p);
                            Json::obj()
                                .set("phase", p.tag())
                                .set("glyph", f.glyph().to_string())
                                .set("what", f.title())
                                .set("layer", f.layer().tag())
                                .set("taken", f.taken())
                                // The colour the PNG paints this face, so the
                                // browser's terrain layer and `slice map --png`
                                // cannot end up disagreeing about what a slag
                                // bank looks like.
                                .set("colour", css(f.rgb()))
                                .set(
                                    "yields",
                                    f.yields().map(|(i, q)| {
                                        Json::obj().set("item", i).set("perSecond", q as i64)
                                    }),
                                )
                        })
                        .collect(),
                ),
            )
    }
}

/// The valley, once.
///
/// Read down a row rather than across the file: every row is one place, and the
/// three faces on it are a hundred and eighty years of what happened to it.
pub static LAND: &[Feature] = &[
    // ============================================== the ore, and what took it
    Feature {
        tag: "kestrel",
        name: "Kestrel Reach",
        x: 8,
        y: 6,
        w: 14,
        h: 9,
        faces: [
            Face::Ore(400),
            Face::Works,
            Face::Sheds,
        ],
        became: "The richest ore on the map, four hundred a second, a shovel deep. \
                 Kestrel Foundry was put up on top of it in 1951 and the body under \
                 it was finished by 1954. In 2070 the foundry is a shed somebody \
                 stores pallets in.",
    },
    Feature {
        tag: "kestrel-spoil",
        name: "Kestrel Spoil",
        x: 24,
        y: 8,
        w: 9,
        h: 7,
        faces: [
            Face::Ore(180),
            Face::Spent("IronOre"),
            Face::Spent("IronOre"),
        ],
        became: "The same body, on the side nobody built on -- so in 2037 it is not \
                 hidden, it is simply *empty*: open ground, still called Kestrel \
                 Spoil, still iron ore, worth eight a second.",
    },
    Feature {
        tag: "lowfield",
        name: "Lowfield",
        x: 10,
        y: 20,
        w: 10,
        h: 6,
        faces: [
            Face::Ore(220),
            Face::Spent("IronOre"),
            Face::Scrub,
        ],
        became: "The second body. Worked out in the 1930s, left open, and by 2070 \
                 grown over enough that nothing on the surface says it was ever a mine.",
    },
    // ============================================================== the coal
    Feature {
        tag: "blackband",
        name: "Blackband Seam",
        x: 8,
        y: 30,
        w: 11,
        h: 6,
        faces: [
            Face::Coal(800),
            Face::Coal(45),
            Face::Spent("Coal"),
        ],
        became: "Eight hundred a second of it at the outcrop in 1890, with nobody \
                 within a hundred years who wants any. The easy coal went first: in \
                 2037 there is still a seam here and it is forty-five a second, which \
                 will just hold a fracture open and will not run a district. By 2070 \
                 it is finished. This one row is most of why the two centuries need \
                 each other.",
    },
    // ============================================================= the water
    Feature {
        tag: "millrace",
        name: "Mill Race",
        x: 2,
        y: 40,
        w: 34,
        h: 5,
        faces: [
            Face::River(1_600),
            Face::River(1_200),
            Face::River(500),
        ],
        became: "The same river in all three, and less of it each time: culverted \
                 under the district in 2019, and by 2070 what is not in the culvert \
                 has been abstracted for something upstream. It still turns a wheel \
                 in all three, which makes it the only thing on this map that is both \
                 true in every century and useful in every century.",
    },
    // ======================================================== what grows here
    Feature {
        tag: "oakshaw",
        name: "Oakshaw",
        x: 40,
        y: 4,
        w: 14,
        h: 10,
        faces: [
            Face::Forest,
            Face::Scrub,
            Face::Scrub,
        ],
        became: "Oak, and the reason a 1890 machine has a timber frame at all. Felled \
                 for pit props and never replanted.",
    },
    Feature {
        tag: "longwood",
        name: "Long Wood",
        x: 40,
        y: 18,
        w: 12,
        h: 8,
        faces: [
            Face::Forest,
            Face::Street,
            Face::Street,
        ],
        became: "The other wood. Tarrant Street runs through where it was, and has \
                 since the works needed somewhere to put its people.",
    },
    // ============================================================ the routes
    Feature {
        tag: "drove",
        name: "The Drove",
        x: 0,
        y: 16,
        w: 72,
        h: 2,
        faces: [
            Face::Track,
            Face::Road,
            Face::Road,
        ],
        became: "A cart track, then a metalled road, on the same line across the \
                 valley -- because the valley is the same shape and there was only \
                 ever one sensible way through it.",
    },
    Feature {
        tag: "cut",
        name: "The Cut",
        x: 0,
        y: 28,
        w: 72,
        h: 1,
        faces: [
            Face::Bare,
            Face::Track,
            Face::Track,
        ],
        became: "Nothing at all in 1890. The railway came in 1904 and is still the \
                 only way anything heavy leaves the district.",
    },
    // ================================================= what 2037 put up here
    Feature {
        tag: "castings",
        name: "Tarrant Castings",
        x: 54,
        y: 30,
        w: 10,
        h: 6,
        faces: [
            Face::Bare,
            Face::Table(500),
            Face::Table(24),
        ],
        became: "A continuous caster, which is the one thing on this map that 2037 \
                 has and 1890 could not have imagined: iron in a usable shape, \
                 coming out of a machine rather than out of the ground.",
    },
    Feature {
        tag: "switchyard",
        name: "The Switchyard",
        x: 54,
        y: 20,
        w: 12,
        h: 6,
        faces: [
            Face::Bare,
            Face::Sheds,
            Face::Sheds,
        ],
        became: "Sheds, cranes and a weighbridge. This is what `modern logistics` \
                 looks like on the ground, and it is why a train may be loaded in \
                 2037 and a cart may not in 1890.",
    },
    Feature {
        tag: "pylonline",
        name: "The Pylon Line",
        x: 0,
        y: 2,
        w: 72,
        h: 1,
        faces: [
            Face::Bare,
            Face::Pylons,
            Face::Pylons,
        ],
        became: "The grid, drawn as the thing it actually is: a line of steel across \
                 the top of the valley that was not there in 1890 and is the reason \
                 a motor works in 2037.",
    },
    Feature {
        tag: "chimney",
        name: "The Chimney",
        x: 30,
        y: 32,
        w: 3,
        h: 3,
        faces: [
            Face::Bare,
            Face::Tower,
            Face::Tower,
        ],
        became: "Built in 1951 with the foundry, listed in 2033, and therefore still \
                 standing in 2070 with nothing left to take smoke away from.",
    },
    Feature {
        tag: "slagbank",
        name: "The Slag Bank",
        x: 22,
        y: 34,
        w: 7,
        h: 5,
        faces: [
            Face::Bare,
            Face::Slag,
            Face::Slag,
        ],
        became: "Open ground in 1890, and a hill in 2037 -- made entirely of what the \
                 foundry did not want. Nothing grows on it in either later phase.",
    },
    // ============================================================ the rift
    Feature {
        tag: "rift",
        name: "The Rift",
        x: 62,
        y: 8,
        w: 6,
        h: 6,
        faces: [
            Face::Rift,
            Face::Gantry,
            Face::Gantry,
        ],
        became: "The one feature here that is not a natural object in any phase and \
                 is in all three of them. In 1890 it is a place where the light is \
                 wrong and the birds will not go. In 2037 there is a gantry over it \
                 with a serial number on the side.",
    },
];

/// A face's colour, as the browser wants it.
fn css(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

pub fn feature(tag: &str) -> Option<&'static Feature> {
    LAND.iter().find(|f| f.tag == tag)
}

/// What is at one tile in one phase.
pub fn face_at(x: i32, y: i32, p: Phase) -> Face {
    // Later rows win, so a road drawn across the whole valley does not erase
    // the ore body it crosses -- the ore is authored first and the road after,
    // and anything that must sit on top says so by its position in `LAND`.
    LAND.iter()
        .filter(|f| f.covers(x, y))
        .map(|f| f.face(p))
        .next_back()
        .unwrap_or(Face::Bare)
}

/// Every deposit this phase should be furnished with, in a fixed order.
///
/// Fixed because two clients furnishing the same region in different orders
/// would name its ground differently, and a deposit's name is part of the
/// canonical state.
pub fn seams(p: Phase) -> Vec<(&'static str, &'static Feature, Qty)> {
    LAND.iter()
        .filter_map(|f| f.face(p).yields().map(|(item, q)| (item, f, q)))
        .collect()
}

/// Whatever is already standing on this rectangle in this phase, if anything
/// is.
pub fn taken_by(p: Phase, x: i32, y: i32, w: i32, h: i32) -> Option<&'static Feature> {
    LAND.iter().find(|f| f.face(p).taken() && f.meets(x, y, w, h))
}

// ------------------------------------------------------------- the two maps

/// The whole plot, one character per tile, with a newline at the end of each
/// row.
pub fn glyphs(p: Phase) -> String {
    let mut s = String::with_capacity(((PLOT + 1) * PLOT) as usize);
    for y in 0..PLOT {
        for x in 0..PLOT {
            s.push(face_at(x, y, p).glyph());
        }
        s.push('\n');
    }
    s
}

/// Every glyph that appears in this phase, and what it means.
pub fn legend(p: Phase) -> Vec<(char, &'static str)> {
    let mut seen: Vec<(char, &'static str)> = Vec::new();
    for y in 0..PLOT {
        for x in 0..PLOT {
            let f = face_at(x, y, p);
            if !seen.iter().any(|(g, _)| *g == f.glyph()) {
                seen.push((f.glyph(), f.title()));
            }
        }
    }
    seen.sort_by_key(|(g, _)| *g);
    seen
}

/// The same field as pixels: `scale` pixels per tile, row-major RGB.
pub fn rgb(p: Phase, scale: usize) -> (usize, usize, Vec<u8>) {
    let side = PLOT as usize * scale;
    let mut buf = vec![0u8; side * side * 3];
    for y in 0..side {
        for x in 0..side {
            let c = face_at((x / scale) as i32, (y / scale) as i32, p).rgb();
            let i = (y * side + x) * 3;
            buf[i..i + 3].copy_from_slice(&c);
        }
    }
    (side, side, buf)
}

// ---------------------------------------------------------------- the metric

/// How much of the world a fracture crossing actually changes.
///
/// Experiment 09 measured whether its readability pass had done anything by
/// counting, rather than by looking at two pictures and being pleased. This is
/// the same discipline pointed at the same kind of claim: "crossing a fracture
/// should make the world change clearly" is a sentence somebody can be wrong
/// about, so it gets a number.
#[derive(Clone, Debug, Default)]
pub struct Changed {
    pub tiles: u32,
    pub total: u32,
    /// How many tiles changed, per layer of the world, counted on the layer
    /// they *left*.
    pub by_layer: Vec<(Layer, u32)>,
    /// Features whose face is different, and what happened to them.
    pub features: Vec<&'static str>,
}

impl Changed {
    pub fn pct(&self) -> u64 {
        if self.total == 0 {
            0
        } else {
            self.tiles as u64 * 100 / self.total as u64
        }
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("tiles", self.tiles as i64)
            .set("total", self.total as i64)
            .set("pct", self.pct() as i64)
            .set(
                "byLayer",
                Json::Arr(
                    self.by_layer
                        .iter()
                        .map(|(l, n)| Json::obj().set("layer", l.tag()).set("tiles", *n as i64))
                        .collect(),
                ),
            )
            .set(
                "features",
                Json::arr(self.features.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
            )
    }
}

/// What is different between two phases of the same ground.
pub fn changes(a: Phase, b: Phase) -> Changed {
    let mut c = Changed { total: (PLOT * PLOT) as u32, ..Default::default() };
    let mut layers: std::collections::BTreeMap<Layer, u32> = std::collections::BTreeMap::new();
    for y in 0..PLOT {
        for x in 0..PLOT {
            let (fa, fb) = (face_at(x, y, a), face_at(x, y, b));
            if fa != fb {
                c.tiles += 1;
                *layers.entry(fb.layer()).or_default() += 1;
            }
        }
    }
    c.by_layer = layers.into_iter().collect();
    c.features = LAND.iter().filter(|f| f.face(a) != f.face(b)).map(|f| f.tag).collect();
    c
}

pub fn to_json() -> Json {
    Json::obj()
        .set("plot", PLOT as i64)
        .set("features", Json::Arr(LAND.iter().map(|f| f.to_json()).collect()))
}
