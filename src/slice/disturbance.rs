//! One compact encounter in the player's factory. Only deliveries charge it;
//! warnings, movement and damage are derived on the settlement clock. Damage
//! enters the affected room as its ordinary, replayable connection command.
use crate::json::Json;
use crate::model::Tick;
use crate::mp::world::Conn;
use crate::mp::{as_secs, secs};

pub const LIMIT: u64 = 40_000;
pub const WARNING: Tick = secs(25);

#[derive(Clone, Debug)]
pub struct Incident {
    pub region: &'static str,
    pub connection: Conn,
    pub start: Tick,
    pub impact: Tick,
    pub damaged: bool,
    pub from: (i32, i32),
    pub to: (i32, i32),
}

#[derive(Clone, Debug, Default)]
pub struct Disturbance {
    pub charge: u64,
    pub last_delivery: Tick,
    pub incident: Option<Incident>,
    pub cooldown: Tick,
    pub resolved: u64,
}

impl Disturbance {
    pub fn delivered(&mut self, region: &str, item: &str, qty: u64, at: Tick) {
        if region == "district" && item == "IronOre" && qty > 0 {
            self.last_delivery = at;
            if at >= self.cooldown && self.incident.is_none() {
                self.charge = self.charge.saturating_add(qty);
            }
        }
    }
    pub fn settle(&mut self, at: Tick) {
        self.incident = None;
        self.charge = 0;
        self.cooldown = at + secs(120);
        self.resolved += 1;
    }
    pub fn to_json(&self, at: Tick) -> Json {
        Json::obj().set("charge", self.charge as i64).set("limit", LIMIT as i64)
            .set("resolved", self.resolved as i64)
            .set("incident", self.incident.as_ref().map(|i|
                Json::obj().set("region", i.region).set("from", Json::arr(vec![i.from.0,i.from.1]))
                    .set("to", Json::arr(vec![i.to.0,i.to.1]))
                    .set("source", i.connection.from as i64).set("target", i.connection.to as i64)
                    .set("item", i.connection.item.clone()).set("start", i.start)
                    .set("impact", i.impact).set("damaged", i.damaged)
                    .set("secondsLeft", as_secs(i.impact.saturating_sub(at)))
            )).set("cohorts", self.incident.is_some() as i64)
    }
}
