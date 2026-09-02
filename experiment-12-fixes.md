# Notes from play session

1. I don't like the fact the ore mine's and water intakes aren't buildings themselves, we need to explore a "machine" variant here
2. Connecting everything to bays then to machines is logical but it isn't interesting, it feels like work, belts and trains only work between bays (and yards), maybe each machine has a requirement for an internal bay? This is especially annoying for power that needs to go to a bay
3. Each thing in a room should have its input and output stats visible in the inspector, inspector should also update on hover not just click, the requirement and current input should show
5. We had a desync issue when a player refreshed the browser, or clicked the left hand menu (we should create a player id in local storage to allow rejoins)
6. We need a better tool pallete for wires, its hidden in the scroll, i think a floating window in the game screen and if I click a machine (or any building) it should give connection options 
7. pre built machines takes the fun out of the game entirely
8. pre placed input and outputs dont make too much sense, maybe a different ui indicator for required outputs
9. we completed a goal that passes the room (power requirement in coal basin) we then disconnected one of the power stations and the total power out per second dropped, after reconnecting it it didnt pick up again to complete the goal again (i think this was fixed by switching rooms)
10. there should be a way to see a rooms' input (we didnt know if power was coming to the iron valley from coal basin) we eventually figured out that the pre placed yard was where the coal was being shipped
11. connecting a bay to a machine gives an option of type, this could be 1 less click if the bay only has 1 item type in it, connections should also show the item type they're moving, visually
12. restoring a building that was deleted doesnt restore its connections
13. lines in the room should be physical, they can still be auto drawn (but they take up space so cant overlap), machines with connection point locations based on their internals would make this easier, this could solve number 12 above, auto drawn connections should use some sort of square connection instead of the straight lines.
14. after buiding in iron valley and switching rooms i couldnt switch back, not sure if bug or lag
15. the freezing for one of the players kept happening, we should probably have a heartbeat tick that syncronises clients to the current latest tick
16. we got a red message in the right hand display on map view saying coal could not be delivered, but I couldn't see if the coal was just disappearing becuase the train kept moving it
17. overall the visuals need to be improved, especially room view

# Server Error logs (could be related to above I wasn't monitoring it)

request failed: stream did not contain valid UTF-8
request failed: stream did not contain valid UTF-8
request failed: An existing connection was forcibly closed by the remote host. (os error 10054)
request failed: stream did not contain valid UTF-8
request failed: stream did not contain valid UTF-8
request failed: stream did not contain valid UTF-8
request failed: stream did not contain valid UTF-8