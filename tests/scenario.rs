//! The rules are guesses. They still have to be the *same* guess twice.
//!
//! `scenario.rs` is the only part of this crate that is allowed to be wrong on
//! purpose -- what a smelter costs is a design decision and will change. What
//! it may not be is inconsistent: two clients holding one log must agree on
//! what was spent and who won, or a scoreboard is decoration.
//!
//! These tests do not check that the numbers are good. They check that the
//! numbers mean something.

use temporal_rooms::graph::Graph;
use temporal_rooms::live::{Command, Edit, Log};
use temporal_rooms::model::Tick;
use temporal_rooms::scenario::{self, Order};
use temporal_rooms::{dsl, live};

fn scenario(path: &str) -> scenario::Scenario {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    scenario::parse(&src).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn log_of(sc: &scenario::Scenario) -> Log {
    let path = format!("configs/{}", sc.plant);
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let prog = dsl::parse(&src).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut g = Graph::from_program(&prog);
    g.apply_positions(&src);
    Log::new(g)
}

fn eval(sc: &scenario::Scenario, log: &Log, t: Tick) -> temporal_rooms::json::Json {
    scenario::evaluate(sc, log, t).unwrap_or_else(|e| panic!("t={t}: {}", e.msg))
}

fn retune(log: &Log, name: &str, at: Tick, f: impl FnOnce(&mut temporal_rooms::graph::Node)) -> Command {
    let mut n = log.base.node(name).unwrap_or_else(|| panic!("no `{name}`")).clone();
    f(&mut n);
    Command { at, edit: Edit::Retune(n) }
}

/// Every scenario on disk parses, names a plant that exists, and asks for
/// something.
#[test]
fn the_scenarios_on_disk_are_answerable() {
    let mut found = 0;
    for e in std::fs::read_dir("scenarios").expect("no scenarios/ directory").flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "scenario") {
            continue;
        }
        found += 1;
        let sc = scenario(p.to_str().unwrap());
        assert!(!sc.orders.is_empty());
        assert!(sc.budget > 0, "`{}` gives the player no money", sc.name);
        let log = log_of(&sc);
        // Every item an order names has to be something a sink here consumes,
        // or the order is unmeetable by construction.
        for o in &sc.orders {
            let item = o.item();
            assert!(
                log.base.items.iter().any(|i| i == item),
                "`{}` orders `{item}`, which `{}` has never heard of",
                sc.name,
                sc.plant
            );
        }
        let j = eval(&sc, &log, 1_000);
        assert_eq!(j.at("won").as_bool(), Some(false), "`{}` is won at t=1,000", sc.name);
    }
    assert!(found >= 1, "no scenarios were found to check");
}

/// The plant a scenario hands you does not solve it. That is the whole point,
/// and it is exactly the kind of thing that quietly stops being true when
/// somebody retunes the config.
#[test]
fn the_scenario_is_not_already_solved() {
    let sc = scenario("scenarios/first-gears.scenario");
    let log = log_of(&sc);
    let deadline = sc.orders[0].deadline();
    let j = eval(&sc, &log, deadline);
    assert_eq!(j.at("won").as_bool(), Some(false), "the starting plant already meets the order");
    assert_eq!(j.at("lost").as_bool(), Some(true), "a missed deadline was not called missed");
    let o = &j.at("orders").as_arr()[0];
    let have = o.at("have").as_u64().unwrap();
    let need = o.at("need").as_u64().unwrap();
    // Close enough to be a puzzle, far enough to be a problem.
    assert!(have * 2 < need * 2, "{have} of {need}");
    assert!(have * 3 > need, "the starting plant delivers {have} of {need}: that is not a puzzle");
}

/// And it is solvable, inside the budget, by an edit made while it runs.
#[test]
fn the_scenario_is_solvable_within_budget() {
    let sc = scenario("scenarios/first-gears.scenario");
    let mut log = log_of(&sc);
    log.commands.push(retune(&log.clone(), "GearPress", 2_000, |n| n.count = 3));
    let deadline = sc.orders[0].deadline();
    let j = eval(&sc, &log, deadline);
    assert_eq!(j.at("won").as_bool(), Some(true), "two more presses did not deliver the order");
    let spent = j.at("spent").as_u64().unwrap();
    assert!(spent <= sc.budget, "the fix costs {spent} out of a budget of {}", sc.budget);
    assert!(j.at("overspent").is_null());
}

/// Money is spent when it is spent, and the ledger says on what.
#[test]
fn every_purchase_appears_on_the_receipt() {
    let sc = scenario("scenarios/first-gears.scenario");
    let mut log = log_of(&sc);
    let base = log.clone();
    log.commands.push(retune(&base, "GearPress", 1_000, |n| n.count = 2));
    log.commands.push(retune(&base, "Rail", 2_000, |n| n.count = 3));

    let j = eval(&sc, &log, 3_000);
    let bought = j.at("purchases").as_arr();
    assert_eq!(bought.len(), 2, "{bought:?}");
    assert_eq!(bought[0].at("at").as_u64(), Some(1_000));
    assert_eq!(bought[0].at("cost").as_u64(), Some(sc.costs.process));
    assert_eq!(bought[1].at("at").as_u64(), Some(2_000));
    assert_eq!(bought[1].at("cost").as_u64(), Some(sc.costs.link * 2));
    assert_eq!(j.at("spent").as_u64(), Some(sc.costs.process + sc.costs.link * 2));

    // Nothing has been bought yet at a tick before the first purchase.
    assert_eq!(eval(&sc, &log, 999).at("spent").as_u64(), Some(0));
}

/// Demolition refunds nothing, and buying the same thing back costs again.
/// A harsh rule, and a rule.
#[test]
fn taking_something_down_does_not_refund_it() {
    let sc = scenario("scenarios/first-gears.scenario");
    let mut log = log_of(&sc);
    let base = log.clone();
    log.commands.push(retune(&base, "GearPress", 1_000, |n| n.count = 4));
    log.commands.push(retune(&base, "GearPress", 2_000, |n| n.count = 1));
    log.commands.push(retune(&base, "GearPress", 3_000, |n| n.count = 4));
    let j = eval(&sc, &log, 4_000);
    assert_eq!(
        j.at("spent").as_u64(),
        Some(sc.costs.process * 6),
        "three presses bought twice should cost six presses"
    );
}

/// Spending past the budget is reported, at the command that did it, and every
/// client that replays the log finds the same command.
#[test]
fn going_over_budget_names_the_command_that_did_it() {
    let sc = scenario("scenarios/first-gears.scenario");
    let mut log = log_of(&sc);
    let base = log.clone();
    log.commands.push(retune(&base, "GearPress", 1_000, |n| n.count = 3));
    // Far more rail than anyone could afford.
    log.commands.push(retune(&base, "Rail", 5_000, |n| n.count = 200));
    let j = eval(&sc, &log, 9_000);
    assert_eq!(j.at("overspent").as_u64(), Some(5_000));
    assert!(j.at("spent").as_u64().unwrap() > sc.budget);
    // And the spend is a pure function of the log: asking twice agrees.
    assert_eq!(sc.spend(&log, 9_000).unwrap().total, sc.spend(&log, 9_000).unwrap().total);
}

/// A sustained rate cannot be met out of a warehouse, which is the only reason
/// the order kind exists.
#[test]
fn a_sustained_order_is_about_the_plant_and_not_the_buffer() {
    let sc = scenario("scenarios/steady-gears.scenario");
    let sustain = sc
        .orders
        .iter()
        .find(|o| matches!(o, Order::Sustain { .. }))
        .expect("steady-gears has no sustained order");
    let Order::Sustain { qty, per, from, to, .. } = sustain else { unreachable!() };
    let log = log_of(&sc);
    let j = eval(&sc, &log, *to);
    let o = j.at("orders").as_arr().iter().find(|o| o.at("text").as_str() == Some(&sustain.text()))
        .expect("the sustained order vanished");
    // The requirement is the rate times the window, and not a total that a
    // large enough bay could have been sitting on since tick zero.
    assert_eq!(o.at("need").as_u64(), Some(qty * (to - from) / per));
    assert!(o.at("have").as_u64().unwrap() < o.at("need").as_u64().unwrap());
}

/// Delivery is what leaves through a sink, not what any machine happens to
/// swallow. `GearPress` eats plates; that is not a delivery of plates.
#[test]
fn delivery_is_what_leaves_through_a_sink() {
    let sc = scenario("scenarios/first-gears.scenario");
    let log = log_of(&sc);
    let t = 40_000;
    let (gears, plates, consumed_plates) = live::with_state(&log, t, |a| {
        (
            scenario::delivered(&a, "Gear"),
            scenario::delivered(&a, "IronPlate"),
            a.room.counters().consumed
                [a.prog.items.iter().position(|i| i == "IronPlate").unwrap()],
        )
    })
    .unwrap();
    assert!(gears > 0, "nothing was delivered at all");
    assert!(consumed_plates > 0, "the presses never ate a plate");
    assert_eq!(plates, 0, "plates eaten by a press were counted as plates delivered");
}

/// Costs are per member, because a population is what a placed object stands
/// for.
#[test]
fn a_population_is_priced_as_a_population() {
    let sc = scenario("scenarios/first-gears.scenario");
    let mut one = log_of(&sc).base;
    let mut ten = one.clone();
    for n in &mut one.nodes {
        if n.name == "Smelter" {
            n.count = 1;
        }
    }
    for n in &mut ten.nodes {
        if n.name == "Smelter" {
            n.count = 10;
        }
    }
    assert_eq!(
        sc.costs.of_graph(&ten) - sc.costs.of_graph(&one),
        sc.costs.process * 9,
        "nine more smelters did not cost nine smelters"
    );
}

/// A scenario file that says something impossible says so, rather than
/// producing a game with no rules in it.
#[test]
fn a_broken_scenario_file_is_refused() {
    let bad: &[(&str, &str)] = &[
        ("scenario A {\n  budget 10\n  order deliver 1 X by 2\n}", "which plant"),
        ("scenario A {\n  plant p1-gears.factory\n}", "asks the player for nothing"),
        (
            "scenario A {\n plant p.factory\n cost teapot 3 each\n order deliver 1 X by 2\n}",
            "nothing called `teapot` has a price",
        ),
        (
            "scenario A {\n plant p.factory\n order juggle 1 X by 2\n}",
            "`juggle` is not a kind of order",
        ),
        ("plant p.factory\n", "outside any scenario block"),
    ];
    for (src, wanted) in bad {
        let e = scenario::parse(src).expect_err(&format!("accepted:\n{src}"));
        assert!(e.contains(wanted), "refused with `{e}`, wanted `{wanted}`");
    }
}

/// And a good one round-trips its meaning, not merely its bytes.
#[test]
fn a_scenario_file_says_what_it_looks_like_it_says() {
    let sc = scenario("scenarios/steady-gears.scenario");
    assert_eq!(sc.plant, "p1-gears.factory");
    assert_eq!(sc.orders.len(), 2);
    assert_eq!(sc.orders[0], Order::Deliver { qty: 8_000, item: "Gear".into(), by: 40_000 });
    assert_eq!(
        sc.orders[1],
        Order::Sustain { qty: 20, per: 100, item: "Gear".into(), from: 40_000, to: 60_000 }
    );
    assert_eq!(sc.costs.storage_per, 1_000);
}
