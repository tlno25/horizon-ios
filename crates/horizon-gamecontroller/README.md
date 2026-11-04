# Horizon iOS GameController bridge

Apple GameController snapshots for the iOS game, isolated from the safe Rust gameplay code.
Adapted from the MIT-licensed `crates/horizon-gamecontroller` implementation in
[heyskylo/horizon-ios](https://github.com/heyskylo/horizon-ios) (2026-11-02).
The original project's license and copyright notices remain applicable.

On non-iOS targets, hardware APIs are no-ops so the source can be tested on Linux.
On iOS, `objc2-game-controller` reads `GCController` without using a desktop
controller backend. The iOS binary converts snapshots into the existing
shared `PadState`; no Apple object references cross into game simulation.
