 i have an experiment i want to do, lets say i have a factory simulator:
SOURCE
Produces:
100 IronOre every 60 ticks

PROCESSOR
Consumes:
10 IronOre

After:
20 ticks

Produces:
10 IronPlate

STORAGE
Capacity:
1000 units

constructed like this

Source -> Storage -> Processor -> Storage

I need a DSL to represent how i would simulate this in an event based manner, as well as a way to mathematically calculate the end result after x ticks without simulating each tick

write an example implementation using a typesafe performant language that lends itself well to this concept and test it with 3 different configurations in increasing scales, our eventual goal is to support billions of factory objects
