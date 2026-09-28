# TAB plugin for Pumpkin — plan

Status: **planning, nothing built.** This file is the source of truth so the work
survives a fresh session.

Related: `PERM-PLAN.md` (separate plugin, separate session).

## The bug found on 2026-09-28

`/papicheck` was still registered under the old name. The rename to PIE caught
`/papi` only where it had a leading slash, so the bare literal in
`Command::new(&["papicheck"...])` was missed while the doc comments above it were
converted. The command is now `/piecheck`. **The register-a-command literal is not
covered by any test** — that is why a rename check missed it.

## Verified against the API (not assumed)

Everything below was read out of the WIT and the host implementation on
Pumpkin 0.2.0+26.3-26.51.

### Available for a connected player

| Need | API | Where |
|:--|:--|:--|
| Custom tab name | `set-tab-list-name` | `player.wit:885` |
| Custom ping | `set-tab-list-latency` / `set-tab-list-ping` | `player.wit:901`, `:1208` |
| Read real ping | `get-ping` | `player.wit:862` |
| Sort order | `set-tab-list-order` | `player.wit:899` |
| Hide from everyone | `set-tab-list-listed` | `player.wit:903` |
| Hide from one viewer | `hide-player` / `show-player` | `player.wit:1199`, `:1201` |
| Header/footer | `broadcast-tab-list-header-footer` | `server.wit:221` |
| Per-player header/footer | `set-tab-list-header-footer` | `player.wit:896` |
| Group membership check | `has-permission-set` -> `option<bool>` | `player.wit:875` |
| Permission level | `has-permission-level` | `command.wit:132` |
| Repeat on interval | `schedule-repeating-task` | `scheduler.wit:18` |
| Join / leave | `player-join-event` / `player-leave-event` | `event.wit:2161-2162` |
| Config file access | `fs.read.data` / `fs.write.data` permissions | `permissions.rs` |
| Legacy colour parse | `from-legacy-string-with-code` | `text.wit` |
| Hex colours | `&x`/`&#RRGGBB` handled by the parser | `pumpkin-util/src/text/legacy.rs` |
| Click / hover | `click-*`, hover events on the component | `text.wit` |
| Rainbow / gradient | `rainbow()`, `gradient()` | `text.wit` |

### Hard limits

- **Fake / offline players: impossible.** The WIT type exists
  (`player-action-add-player`, `c-player-info-update` at `java-packets.wit:1187`)
  but the host serializer has no arm for it. `CPlayerInfoUpdate` appears nowhere
  in the plugin host; `serialize_java_packet` ends in `_ => None`
  (`generated_packets.rs:1002`) and `send-packet` discards that **silently,
  returning `Ok(())`** (`player.rs:3577-3581`). No error, no log.
  Not in v1. Fixing it means a codegen-owned change in `tools/pumpkin-codegen`.
- **No per-viewer tab names.** Every setter goes through
  `world.broadcast_packet_all` (`entity/player.rs:1013`, `:1037`). The whole
  world sees it. Per-viewer *hiding* works, per-viewer *naming* does not.
- **No typed services.** `register_service` / `get_service` are not in the WIT at
  all, so plugin-to-plugin data must go over IPC. This is why the TAB plugin needs
  its own small protocol.

## PIE relationship

- `set_placeholders_batch` **already exists and is live-verified** — the consumer
  resolved 3 lines in one message. No PIE work needed for batching.
- PIE cache is **per-namespace** (single TTL, clamped to 60s), not
  per-placeholder. So per-placeholder refresh intervals must be implemented in the
  TAB plugin, not delegated to PIE.
- **Soft dependency.** No `dependencies` entry, handle `PieError::Unreachable`
  and degrade.

## Decisions taken

1. **Own repository**, built from scratch, MIT licence to match RookTAB.
2. **YAML config.** Java convention — Bukkit's own API is `YamlConfiguration`, and
   LuckPerms / EssentialsX / PlaceholderAPI all use YAML. A TAB config is mostly
   text templates, and JSON cannot hold a single comment. RookTAB uses JSON with
   no comments, which is a usability problem.
3. **Colour: MiniMessage-style tags** (`<#FFFFFF>`, `<red>`) as the primary
   syntax, because the config uses them. Legacy `&` codes accepted too. Note the
   built-in legacy parser handles `&3` and `&x`/`&#RRGGBB` but **not** `<#...>`
   tags, so those need converting before calling `from-legacy-string-with-code`.
4. **Sorting: `set-tab-list-order`.** Do **not** use a `player-list` scoreboard
   objective for it — in vanilla that wins, and it would fight the scoreboard
   feature. Reserve `player-list` as off-limits internally.
5. **Native fallback placeholders.** The plugin resolves a small set itself from
   the API (`get_name`, `get_ping`, `get_world`, online count) so it works
   without PIE, and delegates the rest. Without PIE: real names, real ping, real
   sorting, just no fancy placeholders. Better than literal `%ping%` on screen.
6. **Cross-plugin groups over IPC.** No service registry exists, so the TAB plugin
   defines its own ops. The PERM plugin will push ranks into it.
7. **Refresh: 20 ticks (1s) for the tab list**; per-placeholder intervals for
   expensive placeholders, resolved by a cache inside the TAB plugin.
8. **No offline players, no fake players in v1.**

## Open questions — answer these before writing code

1. **Per-group header/footer?** Java TAB supports a different layout per group.
   Powerful, and most of the complexity. Simplify to one layout for v1?
2. **Does scoreboard ship in v1, or as a follow-up?** Genuinely separate feature.
3. **Animation support?** The example config uses `%animation:Welcome%`,
   `%animation:time%`, `%animation:web%`. That is a whole frame-cycling
   subsystem. In v1, or later?
4. **Bossbar** — in the example config, and `boss-bar.wit` exists. In v1?

## Sizing

RookTAB is ~40 files and covers a *subset* of the above. A faithful v1 of the
full example config is larger than that. Realistic v1:

- config load/save, YAML, comments preserved
- native placeholder fallback (4-6 placeholders)
- PIE batched resolution
- groups via permissions + weight
- sort by weight then alphabetical
- header/footer
- per-player tab name, ping, order
- `/tab reload`

Defer: per-group layouts, animations, bossbar, scoreboard, IPC-for-ranks
(land the permission path first).

## Ordering

1. Skeleton crate + `on_load` that logs, proving the build/link story works.
2. Config load/save with YAML.
3. Native placeholders, no PIE yet — proves the tab list renders.
4. PIE as soft dependency, batched.
5. Groups + sorting.
6. Everything else.

Step 3 before step 4 is deliberate: the tab list should work before PIE exists.
