//! The playable ground. Each feature has one position in the world.
//!
//! `land` retains the historical comparison used by the map study. It is not
//! the source of terrain for a playable region: adjacent fragments are
//! different pieces of ground, not three translations of that study.
use super::{land::{Face, Feature}, Phase};
use crate::json::Json;
use crate::model::Qty;

macro_rules! feature {
    ($tag:literal, $name:literal, $x:expr, $y:expr, $w:expr, $h:expr, $faces:expr, $note:literal) => {
        Feature { tag: $tag, name: $name, x: $x, y: $y, w: $w, h: $h, faces: $faces, became: $note }
    }
}
use Face::*;
pub static LAND: &[Feature] = &[
    feature!("kestrel", "Kestrel Reach", 8,6,14,9, [Ore(400),Bare,Bare], "A broad iron outcrop in the western hills."),
    feature!("kestrel-spoil", "Kestrel Spoil", 24,8,9,7, [Ore(180),Bare,Bare], "The exposed eastern edge of the iron body."),
    feature!("lowfield", "Lowfield", 10,20,10,6, [Ore(220),Bare,Bare], "A smaller ore body below the ridge."),
    feature!("blackband", "Blackband Seam", 8,30,11,6, [Coal(800),Bare,Bare], "Coal at the western outcrop, close to the cart road."),
    feature!("millrace", "Mill Race", 2,40,34,5, [River(1600),Bare,Bare], "A wide, fast reach of the river below the mines."),
    feature!("oakshaw", "Oakshaw", 40,4,14,10, [Forest,Bare,Bare], "Wooded high ground above the valley."),
    feature!("longwood", "Long Wood", 43,21,17,13, [Forest,Bare,Bare], "A belt of woodland along the eastern slope."),
    feature!("drove-west", "The Drove", 0,16,72,2, [Track,Bare,Bare], "The cart road ends at the deep crossing."),
    feature!("district-approach", "Foundry Approach", 72,16,22,2, [Bare,Road,Bare], "The crossing road turns north into the industrial quarter."),
    feature!("foundry-road", "Foundry Road", 91,6,3,30, [Bare,Road,Bare], "A north-south road serving the works, not a repeated valley road."),
    feature!("foundry", "Kestrel Foundry", 99,5,24,11, [Bare,Works,Bare], "The foundry occupies the northern district; its ore comes from across the fracture."),
    feature!("district-spoil", "Old Quarry", 126,8,9,7, [Bare,Spent("IronOre"),Bare], "Eight ore a second remain in this district quarry."),
    feature!("district-lowfield", "Rail Cutting", 128,22,8,6, [Bare,Spent("IronOre"),Bare], "A second worked-out pocket exposed by the railway."),
    feature!("district-coal", "Blackband Adits", 80,30,11,6, [Bare,Coal(45),Bare], "A small coal working under the district's western embankment."),
    feature!("district-water", "Tarrant Canal", 101,25,4,20, [Bare,River(1200),Bare], "The canal runs south through the district, beside the workshops."),
    feature!("castings", "Tarrant Castings", 111,29,10,6, [Bare,Table(500),Bare], "A caster beside the canal provides iron billet."),
    feature!("switchyard", "Freight Sidings", 122,38,14,6, [Bare,Sheds,Bare], "The district's freight sheds sit at the end of the rail spur."),
    feature!("district-grid", "District Substation", 76,3,12,3, [Bare,Pylons,Bare], "The district grid enters from the northwest."),
    feature!("district-rail", "Rail Spur", 94,40,28,1, [Bare,Track,Bare], "A short spur from the workshops to the freight sheds."),
    feature!("zone-road", "Manufacturing Avenue", 144,16,39,2, [Bare,Bare,Road], "The near crossing opens onto the manufacturing avenue."),
    feature!("zone-street", "East Access", 181,16,2,27, [Bare,Bare,Street], "The avenue bends south towards the loading apron."),
    feature!("zone-campus", "Precision Works", 162,25,16,12, [Bare,Bare,Works], "Precision manufacturing occupies the middle of the eastern fragment."),
    feature!("zone-water", "East Culvert", 192,6,4,19, [Bare,Bare,River(500)], "A narrow watercourse follows the eastern boundary."),
    feature!("zone-caster", "Reclaim Caster", 201,30,9,6, [Bare,Bare,Table(24)], "A small reclaim caster, far from the district's iron supply."),
    feature!("zone-coal", "Reclaim Pile", 199,40,8,5, [Bare,Bare,Spent("Coal")], "The last recoverable coal lies in a reclaim pile."),
    feature!("zone-ore", "Old Fill", 187,31,8,6, [Bare,Bare,Spent("IronOre")], "Trace ore remains in the eastern fill."),
    feature!("zone-sheds", "Loading Apron", 185,53,17,7, [Bare,Bare,Sheds], "The eastern loading apron is separate from the precision works."),
    feature!("zone-grid", "East Grid", 207,6,3,17, [Bare,Bare,Pylons], "The eastern grid follows the boundary, away from the district substation."),
    feature!("zone-scrub", "South Embankment", 153,61,25,7, [Bare,Bare,Scrub], "Scrub below the manufacturing zone."),
];

pub fn seams(p: Phase) -> Vec<(&'static str, &'static Feature, Qty)> {
    LAND.iter().filter_map(|f| f.face(p).yields().map(|(i,q)| (i,f,q))).collect()
}
pub fn taken_by(p: Phase, x:i32,y:i32,w:i32,h:i32) -> Option<&'static Feature> {
    LAND.iter().find(|f| f.face(p).taken() && f.meets(x,y,w,h))
}
pub fn to_json() -> Json {
    Json::obj().set("features", Json::Arr(LAND.iter().map(|f| f.to_json()).collect()))
        .set("geography", super::geography::to_json())
}
