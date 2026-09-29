# Design notes

Why PumpkinPIE works the way it does. Nothing here is needed to use it; it is here so the
constraints are recorded somewhere and the reasoning does not have to be rediscovered.

## Why a plugin and not a Pumpkin change

Pumpkin has no PlaceholderAPI equivalent. A tab list plugin, a scoreboard plugin or a chat plugin
that wants `%player_ping%` has to reach into the server API itself, and two plugins that both want
to show a rank end up inventing two incompatible formats.

PumpkinPIE gives them one place to ask. It is a plugin, not a change to Pumpkin, so it installs and
removes without touching the server.

## The model

The provider owns a registry. Plugins that only *consume* placeholders ask it to resolve a string.
Plugins that also *provide* them register a namespace, and the provider calls them back when one of
their placeholders is needed.

```
Consumer plugin                    PumpkinPIE                    Expansion plugin
       |                                  |                                |
       |  set_placeholders                |                                |
       |  "%player_ping% %ranks_prefix%"  |                                |
       |--------------------------------->|                                |
       |                                  |  player_ping: answered here    |
       |                                  |  ranks_prefix: not mine        |
       |                                  |  on_request("ranks", "prefix") |
       |                                  |------------------------------->|
       |                                  |<--------------- "Admin"        |
       |<------------ "42ms Admin"        |                                |
```

The namespace is everything before the **first** underscore. This is Java PlaceholderAPI's rule, and
it is why a namespace may not contain one: `%my_plugin_rank%` would be forwarded to whoever owns
`my`, and it would not look broken.

## Why two crates

Every Pumpkin plugin exports a symbol called `init-plugin`, so a plugin that links both the client
and the provider fails to compile with `duplicate symbol: init-plugin`. There is no link-time
workaround worth having: the linker keeps whichever definition it sees first, and you would silently
ship a component that registers PumpkinPIE's namespaces instead of your own.

| Crate | You want it when |
| :-- | :-- |
| `pumpkin-pie` | You are writing a plugin that resolves placeholders, or provides them. A plain `rlib`. |
| `pumpkin-pie-plugin` | You are building PumpkinPIE itself. The `cdylib` you drop into `plugins/`. |

The split removes the option by making the dependency one-directional. **Depend on `pumpkin-pie`
only.**

## Why unresolved placeholders stay in the text

A placeholder nobody can resolve is left in the text as written — not an error, not an empty string.
This matches Java PlaceholderAPI, and it means a plugin that is momentarily unloaded, or a typo in a
config, shows the user something recognisable rather than a blank.

The visible cost is that you cannot tell "this placeholder does not exist" from "this placeholder
exists but needed a viewer and did not get one" by looking at the text. Both cases are reported in
`unresolved`, which is the field to check when a config is not rendering.

There is no escape for `%`, so `%%player_name%%` resolves to `%Steve%`. Same as Java.

## Why so few built ins

The list is deliberately short, and a placeholder earns a place in it only when it is **vanilla
state a third party has no better source for**. Everything else is meant to arrive as an expansion.
This is the part of the project most likely to upset someone, so the reasoning is written down
here. Asking for one of the following is not a bug report:

- **Ranks, groups, prefixes, economy, balance** — a permission manager or economy plugin owns
  these. They register an expansion.
- **Op level** — superseded by a permission manager. `%player_has_permission:NODE%` is the
  primitive.
- **Scoreboard team, food, saturation, experience** — survival state nobody puts in a tab list.
- **Position and rotation** — `%player_x%`, `%player_y%`, `%player_z%`, `%player_yaw%`,
  `%player_pitch%`. Coordinates change every tick, so caching them is useless, and a plugin that
  wants a location should read the player directly rather than route it through a placeholder API.
- **Client locale, MOTD, difficulty, online percentage, hardcore, online mode, whitelist** — server
  trivia, and the last few change on restart rather than per player.

Two are missing for a technical reason rather than a design one. **`%player_world%`** and
**`%player_display_name%`** are the two most requested. Their getters hand back a host resource with
no way to release it, so they need a change to
[the WIT](https://github.com/Pumpkin-MC/Pumpkin/tree/master/crates/pumpkin-plugin-wit), not to this
plugin.

## Why the callback budget

A resolved line is allowed sixteen expansion callbacks, counted per *distinct* placeholder. This is
a backstop, not a tuning knob: a placeholder that expands into text containing more placeholders, or
two expansions that reference each other, would otherwise recurse. Past the budget the remaining
placeholders are left in the text with a warning in the log.

It is per **line** rather than per message, so a large batch is not penalised for being large. A
placeholder repeated within one line is resolved once, so a line that repeats a value still costs
one round trip.

## Why caching is capped at 60 seconds

`Cache::Ttl` mirrors Java's `Cacheable`: the provider may reuse your values, per viewer, for the TTL
you asked for. It is worth doing — a tab list refreshing once a second asks for the same handful of
placeholders for every player, every second, and most of those values barely move.

The cap exists because the provider cannot know how fast your data goes stale. An hour-long TTL on
a rank is indistinguishable from a bug, so the provider clamps rather than trusting the caller. Read
`Registered::cache` to see what was actually applied instead of assuming you got what you asked for.

## Why the protocol is JSON

It is a set of JSON objects over Pumpkin's inter-plugin IPC, which is part of the public WIT, so
every SDK Pumpkin generates can reach it. A binary format would be faster and would mean every
language reimplemented the encoder.

The cost is that size, not speed: 1 MiB per message, which is generous for placeholder text. The
benefit is that a client in a language with no Pumpkin SDK is a few lines of JSON — see
[protocol.md](protocol.md#implementing-a-client-in-another-language).

## Known rough edges in Pumpkin

Two things outside this plugin fail quietly, and both cost real time while building it. Neither is
this plugin's fault, and neither is versioned or linked here, so treat them as observations about
the build in front of you rather than promises.

- **A permission node that does not start with the plugin's exact name is refused.**
  `Context::register_permission` returns that as an `Err`, which is easy to discard by accident. An
  unregistered node then denies the command to *everyone*, operators included, because
  `has_permission` falls through to a registry lookup that misses. In game the client hides commands
  you cannot use, so the symptom is `Unknown command` with nothing in the log.
- **Config files live in `data/`**, not next to the executable, so an `ops.json` dropped in the
  wrong folder is silently ignored and nobody ends up an operator.
