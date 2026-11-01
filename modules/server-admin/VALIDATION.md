# Validation: 2026-11-01

Executed on the user's Linux machine with GE-Proton11-11 and Enshrouded build 1273043.

- Python dashboard tests: 7 passed (stale-session and inventory checks, command correlation, range validation, result/audit handling, path rejection, Host and Origin restrictions).
- Native adapter tests: 6 checks passed in a compiled Windows executable under Proton, using a fake engine API. These exercise stale-inventory and account rejection, exact-slot removal, partial additions, teleport bounds, and deferred setting acknowledgements. This is not an in-game inventory/teleport test.
- Rust command validation test: passed.
- Existing Rust runtime lifecycle tests: 5 passed.
- Windows runtime and native adapter release builds: succeeded. JavaScript syntax and Python compilation checks passed.
- Isolated dedicated server on UDP 25637: engine and ShroudForge request/reply pings succeeded; 4,207 items enumerated; a completed world save was acknowledged; a world backup was created; an XP multiplier was changed live and restored with confirmed readback.
- Isolated Lua hot reload: a test package's on-load message changed from version 1 to version 2 without restarting the server. The native engine session remained connected. Disabled gameplay mods stayed disabled. Native package reload was correctly rejected.
- Live server installation: original runtime, configuration and world saves backed up; empty server stopped cleanly; administration runtime and engine installed; server restarted through Steam. The dashboard runs through the user service `enshrouded-admin.service` on loopback port 8787.

No player was connected during validation. Live player inventory transactions, client replication of those inventory edits, and native/death-site teleports remain to be tested with an actual connected character. The adapter uses the engine library's transaction and teleport APIs; it does not directly patch inventory counts or player transforms.

Browser automation was unavailable (browser access host unavailable), so the rendered dashboard was not visually inspected. Its HTTP routes, security checks, syntax and local runtime integration were checked.
