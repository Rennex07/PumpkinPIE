# PumpkinPIE

**P**laceholder **I**ntegration **E**ngine for [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin),
modelled on [PlaceholderAPI](https://github.com/PlaceholderAPI/PlaceholderAPI). No code is copied
from it, and the protocol here is its own thing rather than a port.

It answers `%player_name%`, `%player_ping%`, `%server_online%` and friends, and lets other
plugins add their own placeholders by registering an expansion.

```
%player_ping%ms  %player_name%  %server_online%/%server_max_players%
```

- [Status](#status)
- [Why](#why)
- [How it works](#how-it-works)
- [Install](#install)
- [Built in placeholders](#built-in-placeholders)
- [Commands](#commands)
- [Using it from a plugin](#using-it-from-a-plugin)
- [Protocol](#protocol)
- [Differences from Java PlaceholderAPI](#differences-from-java-placeholderapi)
- [Known rough edges](#known-rough-edges-in-pumpkin-and-the-plugin-api)
- [Development](#development)
- [Contributing](#contributing)
- [Thanks](#thanks)

## Status

Early. Protocol version `0`, so the shape may still change.

Verified on **Pumpkin 0.2.0+26.3-26.51, Minecraft Java 26.3 (protocol 777)**, driving the server
over its console so no client is needed. Three components load together: the provider, an
expansion, and a consumer.

The expansion path, via `crates/pumpkin-pie-testexp`:

| Check | Result |
|:--|:--|
| `register_expansion` on two namespaces | accepted, and the applied TTL comes back as asked |
| `on_request` round trip | the expansion's answer reaches `/pie parse` |
| `Cache::Never` | `%testexp_count%` re-asked the expansion on every resolve, incrementing each time |
| `Cache::Ttl` | asked 3 times, called back **once** |
| Declining with `None` | `%testexp_nope%` stayed literal and was reported unresolved |
| Case insensitivity | `%testexp_COUNT%` resolved, reaching the expansion as `name=count` |

The consumer path, via `crates/pumpkin-pie-consumer` and its `/piecheck`, which is the direction
a tab list or scoreboard plugin actually uses:

| Check | Result |
|:--|:--|
| `set_placeholders` | resolved a built in and an expansion, left `%nope_nope%` literal, reported it unresolved |
| `set_placeholders` with no viewer | `%server_online%` resolved, player placeholders stayed empty |
| `set_placeholders_batch` | 3 lines in one message, all resolved, in order |
| `get_placeholder_value` | a built in and an expansion returned values; an unknown id returned `None`, not an error |
| `get_registered_placeholders` | 15 entries across `player`, `server`, `testexp`, `testexpcached` |
| `unregister_expansion` from a plugin that registered nothing | returned empty and left the other plugin's namespaces intact |

The cache survives the IPC boundary, which is the part that only a real consumer can show: three
identical consumer runs asked for `%testexpcached_value%` 3 times and got **1** callback, while
`Cache::Never` placeholders got one callback per request.

Last run was clean: no errors, no warnings from any of the three plugins, no panics. The published
`v0.1.0` artifact was downloaded and loaded to confirm the release itself works.

Still unverified: no real Minecraft client this round, so the player built-ins were exercised with
a name that is not online. No other Pumpkin version has been tried.

## Why

Pumpkin has no PlaceholderAPI equivalent. A tab list plugin, a scoreboard plugin or a chat
plugin that wants `%player_ping%` has to reach into the server API itself, and two plugins that
both want to show a rank end up inventing two incompatible formats.

PumpkinPIE gives them one place to ask. It is a plugin, not a change to Pumpkin, so it installs
and removes without touching the server.

## How it works

The provider owns a registry. Plugins that only *consume* placeholders ask it to resolve a
string. Plugins that also *provide* them register a namespace, and the provider calls them back
when one of their placeholders is needed.

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

This is the shape Java PlaceholderAPI uses:

| Java PlaceholderAPI                   | PumpkinPIE                                   |
|:--------------------------------------|:----------------------------------------------|
| `PlaceholderAPI.setPlaceholders(p, t)` | `set_placeholders`                            |
| `getPlaceholderValue(p, text)`        | `get_placeholder_value` (takes an **id**, not text) |
| `getRegisteredPlaceholders()`         | `get_registered_placeholders`                 |
| `registerPlaceholderExpansion(exp)`   | `register_expansion` (also takes a cache setting) |
| `unregisterPlaceholderExpansion(exp)` | `unregister_expansion`                        |
| `PlaceholderExpansion.getIdentifier()` | the `namespace` you register                  |
| `onRequest(player, identifier)`       | `on_request` (the provider sends this to you) |
| the built in `internal` expansion     | the `player`, `server` and `pie` namespaces  |
| `setPlaceholders` for many players    | `set_placeholders_batch`                      |
| `Cacheable` and a TTL in `config.yml` | `Cache::Ttl` on registration                  |

Rules worth knowing up front:

- **A placeholder is `%namespace_name%`.** The namespace is everything before the first
  underscore, so `%luckperms_prefix%` belongs to whoever registered `luckperms`.
- **`player`, `server` and `pie` are reserved.** The provider answers them, and an expansion
  cannot claim them.
- **Ids are case insensitive.** `%Player_Ping%` and `%player_ping%` are the same placeholder.
- **A placeholder nobody can resolve is left in the text as written.** Not an error, not an
  empty string.
- **There is no escape for `%`.** `%%player_name%%` resolves to `%Steve%`, same as in Java.
- **One call per line, not per placeholder.** Every placeholder the provider cannot answer itself
  costs a round trip, so resolve a whole line at once. A repeated placeholder in one call is only
  resolved once.

## Install

### From a release

Download `pumpkin-pie-<version>.zip` from the [releases page](https://github.com/Rennex07/PumpkinPIE/releases)
and unzip `pumpkin_pie_plugin.wasm` into your Pumpkin server's `plugins/` folder. That is the
whole install. Start the server and you should see:

```
[INFO] PumpkinPIE 0.1.0 ready with 11 built in placeholders
```

**Use a release built against your server.** A component is compiled against a specific
`pumpkin-plugin-api` WIT, and one built against a different one fails to load with
`type-checking export func \`handle-event\``. Each release says which Pumpkin revision it was built
from in its notes.

### From source

Source builds need a local Pumpkin checkout, because the published `pumpkin-plugin-api` crate is
behind the server's WIT and this repository pins the crate to a path.

```bash
# 1. The plugin API the component is built against.
git clone https://github.com/Pumpkin-MC/Pumpkin.git

# 2. This plugin, as a sibling of that checkout.
git clone https://github.com/Rennex07/PumpkinPIE.git
```

So the two directories end up side by side, which is what the `../Pumpkin` in `Cargo.toml`
expects:

```
somewhere/
├── Pumpkin/          <- the server sources
└── PumpkinPIE/      <- this
```

If you keep them elsewhere, edit the `[patch.crates-io]` path in `Cargo.toml`. It points at
`../Pumpkin/crates/pumpkin-plugin-api` and must be present in **your** `Cargo.toml` too if you
are consuming the crate; a patch in this workspace does not apply to yours.

Then:

```bash
rustup target add wasm32-wasip2
cargo build --release --target wasm32-wasip2
```

Copy `target/wasm32-wasip2/release/pumpkin_pie_plugin.wasm` into your Pumpkin server's `plugins/`
folder. (`cargo component build` also works, but plain `cargo build` is enough: the crate is
already a `cdylib` targeting `wasm32-wasip2`.)

Then, in game, as an operator of level 2 or above:

```
/pie parse %player_name% has %player_ping% ms
```

## Built in placeholders

Player placeholders need a viewer. Without one they are **left in the text as written** and
listed in `unresolved`, exactly like an unknown placeholder. So `%player_name%` in text resolved
without a viewer comes back as the literal `%player_name%`, not as an empty string. If you need
one line to work both globally and per player, resolve it per player.

`%player_has_permission%` needs its argument the same way, and is likewise left in the text if
it does not get one.

| Placeholder | Example | Notes |
|:--|:--|:--|
| `%player_name%` | `Steve` | |
| `%player_uuid%` | `069a79f4-...` | |
| `%player_ip%` | `127.0.0.1` | |
| `%player_ping%` | `42` | Milliseconds |
| `%player_gamemode%` | `creative` | |
| `%player_team%` | `red` | Scoreboard team |
| `%player_health%` | `6.5` | |
| `%player_max_health%` | `20` | |
| `%player_has_permission%` | `true` | Takes an argument, see [below](#arguments) |
| `%server_online%` | `5` | |
| `%server_max_players%` | `20` | |

The list is deliberately short, and it is the part of this project most likely to upset someone.
A placeholder earns a place here only when it is **vanilla state a third party has no better
source for**. Everything else is meant to arrive as an expansion.

That rule is why these are missing, and asking for one of them is not a bug report:

- **Ranks, groups, prefixes, economy, balance** — a permission manager or economy plugin owns
  these. They register an expansion.
- **Op level** — superseded by a permission manager. `%player_has_permission:NODE%` is the
  primitive.
- **Scoreboard team, food, saturation, experience** — survival state nobody puts in a tab list.
- **Position and rotation** — `%player_x%`, `%player_y%`, `%player_z%`, `%player_yaw%`,
  `%player_pitch%`. Coordinates change every tick, so caching them is useless, and a plugin
  that wants a location should read the player directly rather than route it through a
  placeholder API.
- **Client locale, MOTD, difficulty, online percentage, hardcore, online mode, whitelist** —
  server trivia, and the last few change on restart rather than per player.

Two are missing for a technical reason rather than a design one. **`%player_world%`** and
**`%player_display_name%`** are the two most requested. Their getters hand back a host resource
with no way to release it, so they need a change to
[the WIT](https://github.com/Pumpkin-MC/Pumpkin/tree/master/crates/pumpkin-plugin-wit), not to
this plugin.

### Arguments

A placeholder can take one argument, written after a colon:

```
%player_has_permission:PumpkinPIE:use%
```

An argument may contain letters, digits and `_ . - : /`. The argument is **only reachable through
`set_placeholders`**, by writing it in the text: `get_placeholder_value` takes an id and no
argument, so asking it for `player_has_permission` always reports `known: false`.

## Commands

All commands need the `PumpkinPIE:use` permission, which is granted to **op level 2 and above**
by default. The node is registered when the plugin loads rather than declared in its metadata,
so it does not show up in a permissions listing.

The node has to be namespaced with the plugin's exact name. `PumpkinPIE:use`, case-sensitive —
`pumpkin-pie:use` is refused, and a node that was never registered denies the command to
*everyone*, operators included.

| Command | What it does |
|:--|:--|
| `/pie parse <text>` | Resolves `<text>` and prints the result |
| `/pie` | Prints the usage line and every placeholder |
| `/pie expansions` | Lists registered expansions and who owns them |

`/pie parse` reports what it could not resolve as a **red error line** and returns a non-zero
result, so it doubles as a way to check a config. Tab completion offers `%id%` values, up to 20
at a time.

## Using it from a plugin

The repository has two crates, and the split is not cosmetic:

| Crate | You want it when |
| --- | --- |
| `pumpkin-pie` | You are writing a plugin that resolves placeholders, or one that provides them. A plain `rlib`. |
| `pumpkin-pie-plugin` | You are building PumpkinPIE itself. The `cdylib` you drop into `plugins/`. |

**Depend on `pumpkin-pie` only.** Every Pumpkin plugin exports a symbol called `init-plugin`, so
a plugin that links both crates fails to compile with `duplicate symbol: init-plugin`. There is no
link-time workaround worth having: the linker keeps whichever definition it sees first, and you
would silently ship a component that registers PumpkinPIE's namespaces instead of your own. The
split above removes the option by making the dependency one-directional.

The crate is not on crates.io yet, so depend on it from git, and **add the same
`[patch.crates-io]` to your own `Cargo.toml`** — a patch in this repository's workspace has no
effect on yours:

```toml
[dependencies]
pumpkin-pie = { git = "https://github.com/Rennex07/PumpkinPIE" }

[patch.crates-io]
pumpkin-plugin-api = { path = "../Pumpkin/crates/pumpkin-plugin-api" }
```

That path is the `../Pumpkin` checkout described in [Install](#install); adjust it to wherever
yours lives. It is required until `pumpkin-plugin-api` is published at a version matching the
server's WIT.

### Consuming placeholders

```rust
use pumpkin_pie::{PieClient, PieError};

fn render() -> Result<(), PieError> {
    let pie = PieClient::new();

    let line = pie.set_placeholders(Some("Steve"), "%player_ping%ms %ranks_prefix%")?;
    println!("{}   unresolved: {:?}", line.text, line.unresolved);

    let online = pie.get_placeholder_value(None, "server_online")?;   // Option<String>
    let all = pie.get_registered_placeholders()?;
    Ok(())
}
```

Every call returns `Result<_, PieError>`. `PieError::Unreachable` is the one to expect on a
first run: it means the provider plugin is not installed, is named differently, or your plugin
messaged itself. Pumpkin reports all three the same way, so check the server log first.

`crates/pumpkin-pie-consumer` is a working plugin that does all of this, so you can read a real
one rather than a fragment. Its `/piecheck` command runs each call and logs what came back.

Refreshing a tab list or a scoreboard for every player, which is what batching exists for:

```rust
let lines = [("Steve", "%player_ping%ms"), ("Alex", "%player_ping%ms")];
let results = pie.set_placeholders_batch(&lines)?;   // one message, not one per player
```

Every batch line carries a viewer. The protocol allows a line with no viewer, but there is no
Rust API for it yet, so a batch is always per player.

### Providing placeholders

This is the `PlaceholderExpansion` equivalent. It is a full plugin: `impl Plugin`, a
`PluginMetadata`, and `register_plugin!`.

```rust
// `IpcMessage` and `PluginId` come from `pumpkin_pie`, not from
// `pumpkin_plugin_api`: the API crate keeps its `wit` module private, so the
// IPC types are not re-exported from there. `pumpkin_pie` re-exports them so
// you need only two imports.
use pumpkin_pie::{answer, Cache, IpcMessage, PieClient, PieError, PluginId};
use pumpkin_plugin_api::{Context, Plugin, PluginMetadata, Result};

pub struct Ranks;

impl Plugin for Ranks {
    fn new() -> Self {
        Self
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "Ranks".to_string(),
            version: "0.1.0".to_string(),
            authors: vec!["you".to_string()],
            description: "Ranks as placeholders.".to_string(),
            dependencies: vec!["PumpkinPIE".to_string()],
            permissions: vec![],
        }
    }

    fn on_load(&self, _context: Context) -> Result<()> {
        self.register()?;
        Ok(())
    }

    fn handle_ipc_message(
        &self,
        _from: PluginId,
        message: IpcMessage,
    ) -> std::result::Result<IpcMessage, String> {
        answer(&message, |ctx| match ctx.name {
            "prefix" => rank_of(ctx.viewer),
            _ => None,
        })
        .map_err(|error| error.to_string())
    }
}

impl Ranks {
    // `Cache::Ttl` lets the provider reuse your values instead of asking every
    // time. The provider clamps it, so a long TTL cannot serve stale ranks.
    fn register(&self) -> Result<(), PieError> {
        let pie = PieClient::new();
        // Read the `Registered` it hands back rather than assuming you got what
        // you asked for: the TTL you request is clamped to 60s, and the provider
        // reports what it actually applied.
        let applied = pie.register_expansion("ranks", &["prefix", "suffix"], Cache::Ttl { ms: 5_000 })?;
        log::info!("ranks registered: {:?}", applied.cache);
        Ok(())
    }
}

fn rank_of(viewer: Option<&str>) -> Option<String> {
    let viewer = viewer?;
    Some(RANKS.get(viewer).copied().unwrap_or("Member").to_string())
}

register_plugin!(Ranks);
```

Four things about that signature are easy to get wrong:

- `handle_ipc_message` returns `std::result::Result`, not `Result`. `pumpkin_plugin_api` exports
  its own `Result` alias, so with it in scope the prelude's `Result` is shadowed and the trait
  signature needs the fully qualified name.
- `answer` returns `Result<Vec<u8>, ProtocolError>`, so the `map_err` is what turns it into the
  `String`-errored result the trait wants.
- `register_plugin!` is a macro from `pumpkin_plugin_api`, not from this crate.
- A namespace may not contain an `_`. The namespace of `%a_b%` is everything before the **first**
  underscore, so a namespace containing one could never be reached — and it would not look
  unreachable, because `%my_plugin_rank%` would quietly be forwarded to whoever owns `my`.

`ctx` is borrowed data only, and `viewer` is a player **name**, not a handle:

| Field | Type | |
|:--|:--|:--|
| `namespace` | `&str` | lowercased |
| `id` | `&str` | the full id as written in the text, so its case is preserved |
| `name` | `&str` | the part after the first `_`, **lowercased** |
| `viewer` | `Option<&str>` | player name, not a `Player` |
| `argument` | `Option<&str>` | the text after the first `:` |

`name` is lowercased to match how names are registered, so `%ranks_PREFIX%` arrives as `prefix` and
a `match` arm for the name you registered will hit. `id` keeps the case it was written with, which
is what you want for echoing a placeholder back to a player.

There is no `Server` or `Player` reachable from inside the closure, so capture whatever else you
need when you build it. Return `None` to decline, and the placeholder stays in the text.

**Do not call back into the provider from inside `resolve`.** The provider asks you synchronously,
from inside its own IPC handler, so reaching for a `PieClient` there re-enters it while it is
still inside the call that invoked you, and the server deadlocks.

List `dependencies = ["PumpkinPIE"]` in your metadata so you are only loaded once the provider
is ready to answer.

### Limits

Each of these fails loudly in the protocol rather than truncating:

| Limit | Value | Exceeding it |
|:--|--:|:--|
| Message size | 1 MiB either way | `too large` |
| One line of text | 32 KiB | that request is refused |
| Namespace, name, viewer, argument | 128 characters | that request is refused |
| Lines per `set_placeholders_batch` | 8192 | the whole batch is refused |
| Expansion callbacks per line | 16 | the rest are left in the text, with a warning in the log |
| Cache TTL an expansion may ask for | 60 s | clamped to 60 s |

The callback budget is per line rather than per message, so a large batch is not penalised for
being large. It is counted per *occurrence*, but a placeholder repeated within one line is only
resolved once, so a line that repeats a value still costs one round trip.

### Caching

An expansion that opts in with `Cache::Ttl` has its values cached per viewer, the way Java's
`Cacheable` works. This is worth doing: on a server of any size, a tab list refreshing once a
second asks for the same handful of placeholders for every player, every second, and most of
those values barely move. The provider caps a TTL at 60 seconds so a plugin cannot serve stale
data indefinitely.

### Other languages

The provider does not care what language you write in. The protocol is a set of JSON objects over
Pumpkin's [inter-plugin IPC][ipc], which is part of the public WIT, so every SDK Pumpkin generates
*can* reach it. Whether a given SDK exposes it conveniently is another matter, and the
`pumpkin-plugin-api` crate keeps its generated `wit` module private, so even in Rust the message
types are re-declared as `PluginId = String` and `IpcMessage = Vec<u8>`. Expect to hand-roll the
export in another language. What that costs you today:

- **There is no client crate but this one.** In another language, a call is a few lines: build the
  JSON, send it, read the reply. A TypeScript or Kotlin client module is the most useful
  contribution here.
- **A reply is a nested result.** The WIT declares `result<result<ipc-message, string>>`, so the
  outer failure means the provider was not reachable and the inner one means it refused.
- **Nothing has a timeout.** A call is synchronous, so an expansion that hangs holds up whoever
  called it. Keep expansions cheap.

[ipc]: https://github.com/Pumpkin-MC/Pumpkin/blob/master/crates/pumpkin-plugin-wit/v0.1/ipc.wit

## Protocol

Every message is a UTF-8 JSON object, at most 1 MiB. A request carries an `op`; a response
carries `ok`, and `error` when `ok` is false. Unknown fields are ignored, so a plugin built
against an older revision keeps working.

### `ping`

```json
-> {"op":"ping"}
<- {"ok":true,"protocol":0,"name":"PumpkinPIE","version":"0.1.0",
    "placeholders":11,"expansions":2}
```

`placeholders` here counts the **built ins only**. `get_registered_placeholders` returns built
ins *plus* every name an expansion declared, so the two numbers will not match.

### `get_registered_placeholders`

```json
-> {"op":"get_registered_placeholders"}
<- {"ok":true,"placeholders":[
      {"id":"player_name","description":"The viewer's name",
       "namespace":"player","source":"PumpkinPIE"}
    ]}
```

### `set_placeholders`

`viewer` is a player name, or omitted.

```json
-> {"op":"set_placeholders","text":"%player_name%","viewer":"Steve"}
<- {"ok":true,"text":"Steve","unresolved":[]}
```

### `set_placeholders_batch`

The same, for many lines at once. This is what a tab list or scoreboard refreshing every player
wants, because it is one message instead of one per player.

```json
-> {"op":"set_placeholders_batch","requests":[
      {"viewer":"Steve","text":"%player_ping%ms"},
      {"text":"%server_online% online"}]}
<- {"ok":true,"results":[
      {"text":"42ms","unresolved":[]},
      {"text":"5 online","unresolved":[]}
    ]}
```

### `get_placeholder_value`

```json
-> {"op":"get_placeholder_value","id":"server_online"}
<- {"ok":true,"known":true,"value":"5"}
```

### `register_expansion`

`placeholders` is what the provider advertises for your namespace in `get_registered_placeholders`
and `/pie`. It is **not** a filter: the provider forwards any `%ranks_*%` to you whether you
listed it or not, which is why the `_ => None` arm in the example above matters. Omit the list,
or leave it empty, if you would rather answer whatever you are asked.

A later call replaces the name list and the cache setting. Names are lowercased on
registration, so `register_expansion("Ranks", &["Prefix"])` works. The response echoes back the
TTL the provider actually applied, after clamping.

```json
-> {"op":"register_expansion","namespace":"ranks","placeholders":["prefix"],
    "cache":{"kind":"ttl","ms":5000}}
<- {"ok":true,"namespace":"ranks","placeholders":["prefix"],
    "cache":{"kind":"ttl","ms":5000}}
```

### `unregister_expansion`

Drops every namespace the calling plugin registered, and anything cached for it.

```json
-> {"op":"unregister_expansion"}
<- {"ok":true,"namespaces":["ranks"]}
```

### `on_request`

Sent by the provider to the plugin that owns the namespace, once per placeholder it cannot answer
itself. `id` is the full placeholder, `name` is the part after the namespace. This is a budgeted
call: one resolved line gets at most sixteen, and anything past that is left in the text, so a
mistake cannot loop forever.

```json
-> {"op":"on_request","namespace":"ranks","id":"ranks_prefix","name":"prefix",
    "viewer":"Steve","argument":null}
<- {"ok":true,"value":"Admin"}
```

## Differences from Java PlaceholderAPI

Worth reading before porting a config over:

- **Placeholder ids may not contain `-` or `.`.** Only letters, digits and `_`. A Java expansion
  using `%server_online-players%` needs renaming.
- **A callback is an IPC message, not a function call.** An expansion that is not loaded, or that
  answers with an error, leaves its placeholder in the text instead of raising.
- **No `%time%`, `%world_*%` or `%random_%`.** They need world and time getters the plugin WIT
  does not expose cheaply.
- **No offline player support.** The viewer has to be online.
- **`get_placeholder_value` takes an id, not a text.** There is no `%placeholder%` string to parse.

## Known rough edges in Pumpkin and the plugin API

Two things outside this plugin fail quietly, and both cost real time while building it:

- **A permission node that does not start with the plugin's exact name is refused.**
  `Context::register_permission` returns that as an `Err`, which is easy to discard by accident.
  An unregistered node then denies the command to *everyone*, operators included, because
  `has_permission` falls through to a registry lookup that misses. In game the client hides
  commands you cannot use, so the symptom is `Unknown command` with nothing in the log.
- **Config files live in `data/`**, not next to the executable, so an `ops.json` dropped in the
  wrong folder is silently ignored and nobody ends up an operator.

Neither is this plugin's fault. Neither is versioned or linked here, so treat them as
observations about the build in front of you rather than promises.

## Development

```bash
cargo test
cargo clippy --target wasm32-wasip2 --all-targets
cargo build --release --target wasm32-wasip2
```

The tests cover the parts that are easy to break quietly: the token scanner, the JSON boundary, and
the expansion table. They need no Minecraft client and no server, so they are the fast feedback
loop. What they cannot check is what a real client sees, which is the one command in
[Install](#install).

Two things are worth knowing before you build:

- **`pumpkin-plugin-api` is patched to a local path.** The root `Cargo.toml` points
  `[patch.crates-io]` at `../Pumpkin`, because the published crate's WIT is behind the server and a
  component built against it fails to load. You need a Pumpkin checkout as a sibling directory, or
  you need to change that line. This is the reason a release build is worth having.
- **Building the whole workspace builds both plugins**, and they cannot be linked together, which
  is the point of the crate split rather than a problem with it.

### Layout

| Path | What lives there |
|:--|:--|
| `crates/pumpkin-pie/src/lib.rs` | Crate docs and re-exports |
| `crates/pumpkin-pie/src/protocol.rs` | The wire format, limits, validation |
| `crates/pumpkin-pie/src/tokens.rs` | Scanning and substituting `%placeholder%` |
| `crates/pumpkin-pie/src/expansion.rs` | Expansions and namespaces |
| `crates/pumpkin-pie/src/builtins.rs` | The built in placeholder table |
| `crates/pumpkin-pie/src/client.rs` | The consumer side, and `answer` for expansion authors |
| `crates/pumpkin-pie/tests/protocol.rs` | Token, protocol and registry tests |
| `crates/pumpkin-pie/tests/readme_example.rs` | Compiles the example above, so the docs cannot rot |
| `crates/pumpkin-pie-plugin/src/lib.rs` | The provider crate's entry point |
| `crates/pumpkin-pie-plugin/src/plugin.rs` | The provider's IPC surface and cache |
| `crates/pumpkin-pie-plugin/src/command.rs` | `/pie`, and the test pinning its permission node |
| `crates/pumpkin-pie-testexp/src/lib.rs` | An expansion: registers namespaces and answers |
| `crates/pumpkin-pie-consumer/src/lib.rs` | A consumer: calls the client API and logs what came back |

The two extra plugins are not decoration. They are the proof that a *different* plugin can link
`pumpkin-pie` and be answered by the provider, which is the whole point of the crate split. They
build in the same workspace with no linker flags, and their `TESTEXP-ON-REQUEST` and
`CONSUMER-CHECK` log lines are how the provider and consumer paths were each verified on a live
server. `testexp` pushes values towards the provider; `consumer` pulls them back out, which is
the direction a real scoreboard or tab list plugin uses.

## Contributing

**Pull requests are welcome.** So are issues, and so is telling me that a placeholder you need
is missing.

Some things that would help:

- **A placeholder you actually want.** `%player_world%` and `%player_display_name%` are the most
  requested. Adding them properly means a change to
  [the WIT](https://github.com/Pumpkin-MC/Pumpkin/tree/master/crates/pumpkin-plugin-wit), which
  would be a contribution to Pumpkin rather than to this repository.
- **Client libraries.** This crate is Rust only. A TypeScript, Kotlin, Go or C# client module
  would make the protocol easier to adopt, and the protocol is small enough to fit in a language
  binding.
- **Tests.** Anything that turns bytes into values or back, and anything that is easy to break
  without noticing.
- **A second implementation.** The protocol is the contract, not this plugin. If you would rather
  write the provider in another language, the protocol version and the operation names are the
  only things consumers depend on. That is what the [Protocol](#protocol) section is for, and it
  is deliberately complete enough to implement against without reading this crate.

If you send a pull request, please say which Pumpkin version you built against and whether you
tested it in game. Bug reports are most useful with the log line the provider printed.

## Thanks

This started because [RookTAB](https://github.com/xRookieFight/RookTAB) wanted `%player_ping%`
and there was nowhere to ask for it. Every design decision here traces back to a plugin that
needed one specific placeholder and could not reach the server.

It was also built by someone who cannot play Minecraft, which turns out to be a useful
constraint. Everything in [Status](#status) was verified by starting a server and typing commands
at it, and two of the bugs that mattered most were invisible to the compiler and to the tests. A
permission node that was never registered looked exactly like a command that did not exist. An
expansion unregistering on unload looked exactly like a clean shutdown, right up until the store
driver reported a trap. Neither would ever have been found by reading the code carefully; both
were found by running the thing and reading the log.

If you use this, or write an expansion for it, or just tell someone about it, that is worth more
than a star. PRs are welcome and so is a note saying which placeholder you had to work around.

Earlier work on this lived under [Epix Development](https://github.com/Epix-Development) and
arrived here renamed. If you can see where the name came from, you have been reading it the way I
wrote it.

## License

GPL-3.0-only, matching the Pumpkin server. See [LICENSE](LICENSE).

One consequence worth knowing: this crate is meant to be depended on, and GPL-3.0 carries over to
anything that copies it. Talking to the provider over IPC does not, because no code is shared, so
a permissively licensed plugin can speak the protocol without touching this repository.
