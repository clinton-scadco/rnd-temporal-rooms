Now I’d stop doing solver archaeology for a bit and build the **first actual game loop**.

Prototype 0 proved three things you needed before that was safe: the UI can round-trip through the DSL, the renderer can stay purely derived from simulation state, and the builder is already flushing out bad abstractions. That’s exactly the point where “more benchmark” starts giving diminishing returns.

The next milestone should be something like **Prototype 1: play a factory, not just inspect one**.

The core loop I’d target is:

```text
place
→ connect
→ run
→ observe bottleneck
→ modify
→ see consequence
→ expand
```

Right now you can build and scrub. The missing ingredient is **pressure**. A reason for the player to care whether the plant works.

I’d add, in roughly this order:

- **Goals/orders**: e.g. deliver `1000 Gear` by tick `50,000`, or sustain `20 Gear/tick` for a window. Nothing fancy. Just enough to turn the simulator from a diagram into a problem.
- **Costs**: machines, links, storage, maybe land/space. Otherwise the answer is “build twelve billion smelters,” which your engine, annoyingly, may actually tolerate.
- **Limited resources**: finite source output, maybe several deposits with different capacities or distances.
- **Failure/feedback UI**: starved, blocked, transport-limited, storage-full, bottleneck. You already have most of this state. Surface it aggressively.
- **Basic upgrade choices**: faster processor vs larger buffer vs more vehicles vs shorter route. This is where your batch/latency model starts producing actual decisions.
- **Save/load plus deterministic command log**: not because save games are glamorous, but because this is the cleanest path toward proving late join and replay later.

I would deliberately **not** add power, fluids, splitters, modules, complicated routing, or recursive line-state compression yet. Those are all valid future systems, but they’re still features looking for a game.

The next technical thing I *would* add is dynamic edits as first-class commands. Right now the builder compiles a document into a plant. For a real game, you want:

```text
tick 12000: place Processor
tick 12020: connect Link
tick 12500: change recipe
tick 13000: upgrade Rail vehicles 8 -> 12
```

and the simulation should be able to invalidate/rebuild only what needs rebuilding. That is much more important now than another asymptotic proof because the player is going to mutate the factory constantly.

So I’d make the next architectural question:

> **Can the simulation stay deterministic and cheap while the player is continuously changing the graph?**

That gives you a very concrete Prototype 1 target:

```text
A small scenario

Ore deposit
   ↓
mine
   ↓
storage
   ↓
smelting
   ↓
gear production
   ↓
delivery target
```

Start with something intentionally underbuilt. The player has maybe a budget of 1000 credits and needs to hit a production target. They can add storage, processors, or transport capacity. Scrubbing the timeline remains available, which is actually a pretty distinctive mechanic/debugging tool.

Then watch how people solve it.

That will tell you far more than another synthetic config about whether your abstractions are fun.

There’s also one feature I’d add immediately because your architecture makes it nearly free: **“why is this not running?”** inspection. Click a machine or installation and show:

```text
Smelter Complex

state: STARVED
needs: 10 IronOre
available locally: 0

upstream:
OreBay empty
next delivery: tick 14,200
Rail utilisation: 100%
```

Similarly for blocked:

```text
BLOCKED
holding: 10 IronPlate
PlateBay: 100% full
downstream consumer utilisation: 42%
```

Factory games live or die on whether the player can understand why their magnificent industrial spaghetti has stopped working.

And after Prototype 1, **then** I’d make the networking proof real:

```text
server:
command log + canonical snapshots

client A:
play normally

client B:
join at tick 80,000
load snapshot
replay commands
hash matches
```

If that works while someone is actively building, deleting and upgrading things, you’ve essentially validated the original technical thesis under actual game conditions.

So the progression I’d use now is:

> **P0:** can we see and manipulate the simulation? Done.  
> **P1:** can we make a tiny factory problem fun to solve?  
> **P2:** can a changing live factory replay/sync exactly?  
> **Then:** start adding richer factory systems.

And keep the workbench. Don’t turn it into the game UI and lose it. It’s already becoming your simulation oscilloscope, and future-you will be deeply irritated if present-you replaces it with tasteful menus and particle effects.