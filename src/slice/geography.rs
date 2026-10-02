//! A single playable landscape. Simulation regions own disjoint patches of it;
//! dates describe the patch, never another copy of the same coordinates.
use super::{land, Phase};
use crate::json::Json;

pub const WIDTH: i32 = land::PLOT * 3;
pub const HEIGHT: i32 = land::PLOT;

pub fn offset(p: Phase) -> i32 { p.index() as i32 * land::PLOT }

// Authored, stepped seams. Both neighbouring patches use the same edge.
pub fn edge(n: usize, y: i32) -> i32 {
    let bend = match y {
        0..=13 => 4,
        14..=23 => 0,
        24..=35 => -4,
        36..=47 => 0,
        48..=71 => 0,
        _ => 0,
    };
    n as i32 * land::PLOT + if n == 0 || n == 3 { 0 } else { bend }
}

pub fn phase_at(x: i32, y: i32) -> Option<Phase> {
    if !(0..WIDTH).contains(&x) || !(0..HEIGHT).contains(&y) { return None; }
    Some(if x < edge(1, y) { Phase::P1890 }
        else if x < edge(2, y) { Phase::P2037 } else { Phase::P2070 })
}

pub fn contains(p: Phase, x: i32, y: i32, w: i32, h: i32) -> bool {
    w > 0 && h > 0 && (y..y+h).all(|row|
        phase_at(x, row) == Some(p) && phase_at(x+w-1, row) == Some(p))
}

pub fn polygon(p: Phase) -> Json {
    let n = p.index();
    let mut pts = Vec::new();
    for y in [0, 14, 24, 36, 48, HEIGHT] {
        let before = (y-1).max(0);
        pts.push(Json::arr(vec![edge(n, before), y]));
        pts.push(Json::arr(vec![edge(n, y.min(HEIGHT-1)), y]));
    }
    for y in [HEIGHT, 48, 36, 24, 14, 0] {
        let before = (y-1).max(0);
        pts.push(Json::arr(vec![edge(n+1, y.min(HEIGHT-1)), y]));
        pts.push(Json::arr(vec![edge(n+1, before), y]));
    }
    Json::Arr(pts)
}

pub fn to_json() -> Json {
    let patches = super::region::REGIONS.iter().map(|r|
        Json::obj().set("tag", r.tag).set("phase", r.phase.tag())
            .set("offsetX", offset(r.phase) as i64).set("polygon", polygon(r.phase))
    ).collect();
    let crossings = vec![
        Json::obj().set("tag", "deep").set("x", 72).set("y", 17),
        Json::obj().set("tag", "near").set("x", 144).set("y", 17),
    ];
    Json::obj().set("width", WIDTH as i64).set("height", HEIGHT as i64)
        .set("patches", Json::Arr(patches)).set("crossings", Json::Arr(crossings))
}
