# Discord desktop adapter

Read the root AGENTS.md and docs/discord-presence.md. This directory and the sibling
discord_presence.rs module own optional desktop IPC only. Never depend on or inspect a drawing,
file path, selection, command history or Discord user data. The UI provides settings and receives
status; it owns no transport. Use only Discord's public local RPC specification and permissively
licensed safe Rust platform APIs. Do not add tokens, secrets, authentication dialogs, invites or
assets to simple presence.

Keep one worker and bounded queues/frames/work. Opens, reads and writes must be nonblocking;
shutdown and disable must wake the worker and have bounded cleanup. Windows pipe flushing must
never block or accumulate background workers. Only acknowledge sharing after SET_ACTIVITY is
accepted. Preserve quiet reconnect, rejection reporting, privacy and opt-out persistence.

Required checks: fragmented/coalesced IPC, acknowledgements, ping/pong, disconnects, invalid and
oversized frames, deadlines, blocked writes, bounded clearing/shutdown, preference round trips,
invalid IDs without partial changes, native feature-disabled builds, WASM and cargo xtask ci.
