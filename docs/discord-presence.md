# Discord Rich Presence

The desktop app shares a simple **Designing in CADCraft · Drafting and modeling** activity and
elapsed session time with the locally running Discord desktop app. The fork owner's public
Application ID, `1562973280875315221`, is included. No account login, bot, token, client secret,
public key, game invites or uploaded artwork is required. The displayed application name comes
from that application's name in Discord.

Choose **Tools → Discord Rich Presence…** to switch sharing off or change the Application ID.
Settings survive restarts. Activity is enabled by default in this fork's desktop build. Drawing
names, file paths, geometry, selection and command history are never included. Closing CADCraft
or disabling sharing clears the activity and closes the connection. Discord's own activity
privacy settings still determine whether other people can see it.

Discord must be running on the same computer; the browser version of Discord cannot receive
local IPC. If Discord is closed or restarts, CADCraft quietly retries every 15 seconds. The
settings dialog shows waiting, connecting, sharing, or rejection. CADCraft reports sharing only
after Discord acknowledges `SET_ACTIVITY`. Check the Application ID if Discord rejects it.

The shared UI command is `ui.discord-presence`. JSON calls never open the dialog:

```json
{"method":"engine.execute","params":{"command":"ui.discord-presence","params":{}}}
{"method":"engine.execute","params":{"command":"ui.discord-presence","params":{"settings":{"enabled":false,"applicationId":"1562973280875315221"}}}}
```

These are control-channel request bodies; add `id` and the authenticated control token as
described in [control-protocol.md](control-protocol.md). The settings object replaces the saved
settings. An empty object reads settings, availability and status; menu invocation opens the
dialog. IDs must be decimal strings, 17–20 digits, within `u64`. Secret fields are rejected.

## Module boundary and maintenance

- `apps/cadcraft/src/discord_presence.rs` and `discord_presence/transport.rs` own all IPC. The
  desktop host starts, polls and stops one worker. There is no Discord code in engine/doc/IO.
- `crates/ui-egui/src/discord.rs` owns preferences, validation, the command and dialog. Shared UI
  hooks are one state field, preference save/load, command registration/routing and dialog dispatch.
- The optional desktop `discord-presence` Cargo feature is enabled by default. Build with
  `--no-default-features --features contractor` for Contractor without Discord, or
  `--no-default-features` for neither extension. CLI and browser builds have no Discord IPC.
- Windows uses safe `interprocess` APIs (0BSD OR Apache-2.0); Unix uses safe `socket2` APIs
  (MIT OR Apache-2.0) and standard Unix streams. No proprietary SDK or unsafe code is added.
- One bounded worker owns nonblocking local IPC, with 64 KiB frames, a four-frame output queue,
  bounded work per tick, five-second reply deadlines and 30-second ping/pong heartbeats. Settings
  changes and shutdown wake it immediately. A 200 ms grace period bounds clearing on disconnect;
  Windows pipe drops never spawn background flush workers.
- Windows `PIPE_NOWAIT` byte reads may return zero while idle; the transport normalizes these
  to `WouldBlock`. The library also maps closed-pipe reads to zero, so ping writes and bounded
  response deadlines detect disconnects there. A real Windows pipe regression covers idle reads,
  data delivery and a closed peer's failed heartbeat write.
- Protocol reference: [Discord RPC over IPC](https://discord.com/developers/docs/topics/rpc).
  Endpoint discovery, framing, handshake and `SET_ACTIVITY` use that public specification.

After upstream updates, check the desktop lifecycle hooks and UI command/persistence hooks,
then run focused Discord protocol/preferences tests, native feature-disabled checks, and
`cargo xtask ci`. Inspect the actual settings window and verify Discord acknowledges the
activity. Tests use synthetic protocol peers and never publish test activity to Discord.
