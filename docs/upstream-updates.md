# Contractor fork upstream updates

## 2026-10-28 — Contractor v1.3.1 drawing stability

Contractor v1.3 could exit while drawing or deleting in an existing drawing. The captured
stack trace identifies `rstar` 0.13's incremental spatial-index insertion during cursor
snapping. This runs during canvas rendering, outside the engine command panic guard.
Overlapping, collinear line bounds can leave the bulk-loaded index with incompatible node
heights after repeated entity-chunk replacement and insertion. The earlier command and
synthetic floor checks did not cover that shape of index update.

Keep CADCraft 0.13.0 and pin the engine's spatial dependency to `rstar =0.12.2`, the version
used in Contractor v1.2. The only API adjustment borrows the query envelope. This retains
incremental indexing, Contractor metadata and Discord's FreeBSD socket fix. Before upgrading
this dependency again, run `crates/engine/tests/spatial_edit_regression.rs`: its synthetic
overlapping-line regression fails with the same panic on 0.13 and passes on 0.12.2. A second
test compares cached and freshly built indexes through drawing, erasing, undo and redo.
No private drawing or project-derived fixture is committed.

The optimized Windows desktop (`cadcraft 0.13.0 (contractor-v1.3.1)`) passes the exact
interactive drawing/deleting sequence that exited v1.3 on a separate copy of the reported
DWG. Deleting eight existing lines with the Delete key, undo/redo and switching through
Plan, Engineering and 3D also pass. Both the original file and its test copy retain their
original hashes. Empty and dense synthetic drawings pass the same creation/deletion checks.

The synthetic 31-member floor passes individual piling-depth edits without changing other
members, undo, and native DWG save/reopen with all handles/elevations retained. Plan,
Engineering, 3D and item Properties were inspected; Engineering PDF export succeeds. The
real Discord desktop client acknowledges connection, opt-out and reconnection.

`cargo xtask ci` passes all seven gates: formatting, warning-free native Clippy, tests,
translation reporting, assets, dependency layers and all 12 browser/WASM crate checks.
All 2,507 native tests pass, including the two new spatial regressions and Contractor's
repeated DXF/native DWG round trips. Native desktop/CLI/web/engine/UI/MCP checks pass with
default features disabled; Contractor-only and Discord-only desktop checks pass too.
All nine feature-disabled workspace tests pass, and browser WASM checks pass with Contractor
enabled and disabled. The existing three WASM worker-helper dead-code warnings are unchanged.

## 2026-10-25 — CADCraft 0.13.0 and FreeBSD Discord transport

Integrated the upstream `v0.13.0` release,
[`1fcf654ea8414ea2ef052ca2b3b77411f2026f78`](https://github.com/storytold/cadcraft/commit/1fcf654ea8414ea2ef052ca2b3b77411f2026f78),
141 commits after the fork's 0.11.0 base. This brings upstream's drawing, dimension,
comparison/export, clipboard, file-format, shared UI and dependency updates. Contractor's
model, metadata versions, independent member editing and representation layers are retained.

The shared integration adjustment is workspace snapshot validation: use upstream's new
`docking::validate` adapter while still accepting the optional Contractor tab. Keep upstream's
`getrandom` 0.4 update and the fork's explicit feature forwarding. The new upstream BLOCKREPLACE
command also uses the existing protected-block predicate, so neither JSON nor interactive calls
can detach member metadata from generated geometry. A regression verifies both paths leave the
entire drawing unchanged. Merge progress histories
without attributing Contractor's specialized viewer to general CAD kernel parity.

The three failed nightly FreeBSD runs (39926390990, 40077356397, 40177149893) all stop at the
Discord transport's `Socket` to `UnixStream` conversion. `socket2` does not supply that direct
conversion on FreeBSD with its default features disabled. Transfer the socket through the
standard `OwnedFd` type instead; this is safe, owns the descriptor once, and retains its
nonblocking setting. A Unix regression exercises the actual connection helper using an
isolated temporary socket, checking idle `WouldBlock`, delivery, EOF and bidirectional IO.
Existing Windows pipe behavior is preserved. The FreeBSD workflow now also runs on main pushes
so a desktop adapter regression need not wait for the nightly run.

The exact production transport and its test compile and pass warning-free Clippy checks in an
isolated transport-only harness for both `x86_64-unknown-freebsd` and `aarch64-unknown-freebsd`,
using the locked socket2 version. The native test suite passes all 2,505 tests, including the
new JSON/interactive block-replacement regression and Contractor's repeated DXF/DWG cycles.

The optimized desktop reports `cadcraft 0.13.0 (contractor-v1.3)`. Its actual Discord client
acknowledged activity, opt-out and reconnection. The synthetic 31-member floor retained every
member/elevation through native DWG save/reopen; individual piling-depth edits left other members
unchanged and undo restored the edit. Plan, Engineering, 3D, piling Properties and the Discord
settings window were inspected. The Engineering PDF retained 19 joist axes at 432 points for
288 drawing inches at 1:48, on ANSI D landscape (2448 × 1584 points).

`cargo xtask ci` passes all seven gates: formatting, warning-free native Clippy, tests,
translation reporting, assets, dependency layers and all 12 browser/WASM crate checks. The
existing three WASM worker-helper dead-code warnings are unchanged. Native desktop, CLI, web,
engine, UI and MCP checks pass with default features disabled; Contractor-only and
Discord-only desktop builds also check successfully.
All nine feature-disabled native workspace tests pass, and the browser application checks
successfully for WASM with Contractor both enabled and disabled.

On published code commit `7d3fafc5c9ce9e6e7214b3352011b3ff6442c071`, GitHub's
[Linux quality gate](https://github.com/SleazLee/cadcraft_Contractor/actions/runs/40261311440)
passes all seven steps. Both **FreeBSD 15** jobs pass in
[run 40261311424](https://github.com/SleazLee/cadcraft_Contractor/actions/runs/40261311424):
**x86_64 runs the native tests**, including the real Unix socket and generated-member protection
regressions; **aarch64 cross-compiles the application and test binaries** on x86_64.
Nix, packaging lint and SonarCloud also pass. FreeBSD package validation steps are enabled
for release-branch runs.

Local `plan/` files and `../../craftrules` remain unavailable; the shared instructions' public
GitHub location again returned 404. Root and nested Contractor/Discord instructions were read.

## 2026-10-22 — CADCraft 0.11.0

Integrated `storytold/cadcraft` main through
[`01bbdb24bc27b087893afc37dff07d79862a5b97`](https://github.com/storytold/cadcraft/commit/01bbdb24bc27b087893afc37dff07d79862a5b97),
the latest fetched head on this date: 142 upstream commits since this fork's previous base.
This includes drawing and command improvements, localization updates, file-handling fixes,
authenticated loopback control, and the craft-fonts manifest path/directive security fix.
Contractor metadata and individual member geometry are unchanged by the update.

Dependency conflicts retain both upstream's `craft-docpath`/control-token dependencies and the
fork's feature forwarding/default-feature isolation. Progress documents retain both histories
and distinguish specialized Contractor geometry from general CAD parity. Contractor hooks in
the command registry, transform guards, Properties, workspace persistence, canvas and export
adapters remain isolated as documented in [contractor.md](contractor.md).

The enlarged workspace exceeded Windows' process command-line length limit during the original
all-at-once format check. `cargo xtask ci` now checks formatting one workspace package at a time
on Windows, covering the same source targets. Other operating systems retain the upstream check.
The Unix-only symlink test's helper is referenced inside that test so Windows Clippy does not
reject an unused import. Two upstream long expressions were formatted for the same quality gate.
The upstream year-318857 test now exercises calendar conversion directly instead of constructing
a SystemTime that Windows cannot represent; the extreme-date coverage remains active on Windows.

The local ignored `plan/` instructions and `../../craftrules` checkout remain unavailable.
The shared instructions' public GitHub URL returned 404. Root and nested repository agent notes
were read; no inaccessible shared repository was changed.

Discord transport validation used the owner's running desktop client: handshake reached READY,
SET_ACTIVITY was acknowledged, and clearing the activity was acknowledged. It exposed Windows'
zero-byte idle reads; the transport normalizes them and has a real named-pipe regression covering
idle reads, received data and closed-peer heartbeat writes. No Discord user data was logged.

The portable desktop and CLI use the repository's optimized release profile. Repository build
profiles are unchanged.

## Validation

- `cargo xtask ci`: all six gates passed (formatting, warning-free native Clippy, 2,291 tests,
  assets, dependency layers and all 12 WASM crate checks), including the Windows pipe regression.
- Native desktop/CLI/web/engine/UI/MCP builds passed with default features disabled; Contractor
  and Discord desktop builds also passed independently. Browser WASM builds passed with
  Contractor on and off. All eight feature-disabled desktop workspace tests passed. The existing
  three WASM worker-helper dead-code warnings remain unchanged.
- Final optimized Windows desktop and CLI builds succeeded. Desktop reports
  `cadcraft 0.11.0 (contractor-v1.2)`; CLI reports `cadcraft-cli 0.11.0`.
- The final desktop connected to the actual Discord client, switched to Off, and reconnected.
  Its settings window, Plan, Engineering, 3D and selected-piling Properties were inspected.
- The synthetic 31-member floor retained all members/elevations through native DWG save/reopen.
  Changing one piling's depth left the other members unchanged; undo restored it. Existing
  automated tests also verify repeated DXF/DWG cycles, stacking, joints, copies and selections.
- The Engineering PDF was inspected. ANSI D landscape is 2448 × 1584 points; all 19 joist axes
  measure 432 points for a 288-inch span at 1:48. No project measurements came from screenshots.

## Update checklist

1. Fetch upstream main and record its exact head; merge without replacing Contractor modules.
2. Retain optional feature forwarding and L2 registration; review all shared hooks listed in
   contractor.md and discord-presence.md against their upstream versions.
3. Run Contractor storage/command/UI tests, including native DWG round trips, plus Discord's
   synthetic protocol/preference tests. Run `cargo xtask ci`, native feature-disabled checks,
   and browser WASM checks with Contractor enabled and disabled. Preserve the exact `rstar`
   pin until the overlapping-line spatial-index regression passes on its proposed replacement.
4. Inspect Plan, Engineering, 3D, item Properties and Discord settings. Check native activity
   acknowledgement, opt-out, normal shutdown, and DWG save/reopen/edit.
5. Record validation, commit and push the merge preserving both parent histories. Build and
   package the desktop/CLI from the validated tree; keep earlier portable releases available.
