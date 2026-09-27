# PumpkinPAPI

A placeholder provider for [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin), modelled on
[PlaceholderAPI](https://github.com/PlaceholderAPI/PlaceholderAPI).

It answers `%player_name%`, `%player_ping%`, `%server_online%` and friends, and lets other
plugins add their own placeholders by registering an expansion.

```
%player_ping%ms  %player_name%  %server_online%/%server_max_players%
```

## Status

Early. Protocol version `0`, so the shape may still change. The Python implementation is on
the `python` branch, kept as a second implementation of the same protocol.

## Why

Pumpkin has no PlaceholderAPI equivalent. A tab list plugin, a scoreboard plugin or a chat
plugin that wants `%player_ping%` has to reach into the server API itself, and two plugins that
both want to show a rank end up inventing two incompatible formats.

PumpkinPAPI gives them one place to ask. It is a plugin, not a change to Pumpkin, so it installs
and removes without touching the server.

## How it works

The provider owns a registry. Plugins that only *consume* placeholders ask it to resolve a
string. Plugins that also *provide* them register a namespace, and the provider calls them back
when one of their placeholders is needed.

```
Consumer plugin                    PumpkinPAPI                    Expansion plugin
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

| Java PlaceholderAPI                   | PumpkinPAPI                                   |
|:--------------------------------------|:----------------------------------------------|
| `PlaceholderAPI.setPlaceholders(p, t)` | `set_placeholders`                            |
| `getPlaceholderValue(p, text)`        | `get_placeholder_value`                       |
| `getRegisteredPlaceholders()`         | `get_registered_placeholders`                 |
| `registerPlaceholderExpansion(exp)`   | `register_expansion`                          |
| `unregisterPlaceholderExpansion(exp)` | `unregister_expansion`                        |
| `PlaceholderExpansion.getIdentifier()` | the `namespace` you register                  |
| `onRequest(player, identifier)`       | `on_request` (the provider sends this to you) |
| the built in `internal` expansion     | the `player` and `server` namespaces          |
| `setPlaceholders` for many players    | `set_placeholders_batch`                      |
| `Cacheable` and a TTL in `config.yml` | `Cache::Ttl` on registration                  |

Rules worth knowing up front:

- **A placeholder is `%namespace_name%`.** The namespace is everything before the first
  underscore, so `%luckperms_prefix%` belongs to whoever registered `luckperms`.
- **`player` and `server` are reserved.** The provider answers them, and an expansion cannot
  claim them.
- **Ids are case insensitive.** `%Player_Ping%` and `%player_ping%` are the same placeholder.
- **A placeholder nobody can resolve is left in the text as written.** Not an error, not an
  empty string.
- **There is no escape for `%`.** `%%player_name%%` resolves to `%Steve%`, same as in Java.
- **One call per line, not per placeholder.** Every placeholder the provider cannot answer itself
  costs a round trip, so resolve a whole line at once. A repeated placeholder in one call is only
  resolved once.

## Install

```bash
cargo install cargo-component
```

Build the component and drop it in the server's `plugins` directory:

```bash
cargo component build --release --target wasm32-wasip2
```

Copy `target/wasm32-wasip2/release/pumpkin_papi.wasm` into your Pumpkin server's `plugins/`
folder and start the server. You should see:

```
[INFO] PumpkinPAPI 0.1.0 ready with 17 built in placeholders
```

Then, in game, as an operator:

```
/papi parse %player_name% has %player_ping% ms
```

## Built in placeholders

Player placeholders need a viewer. Without one they resolve to an empty string rather than
failing, so the same text works globally and per player.

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
| `%player_x%` | `1.23` | |
| `%player_y%` | `64` | |
| `%player_z%` | `-8.5` | |
| `%player_yaw%` | `90` | |
| `%player_pitch%` | `-12` | |
| `%player_has_permission:NODE%` | `true` | Takes an argument, see below |
| `%server_online%` | `5` | |
| `%server_max_players%` | `20` | |
| `%papi_version%` | `0.1.0` | |

The list is deliberately short, and it is the part of this project most likely to upset someone.
A placeholder earns a place here only when it is **vanilla state a third party has no better
source for**. Everything else is meant to arrive as an expansion.

That rule is why these are missing, and asking for one of them is not a bug report:

- **Ranks, groups, prefixes, economy, balance** — a permission manager or economy plugin owns
  these. They register an expansion.
- **Op level** — superseded by a permission manager. `%player_has_permission:NODE%` is the
  primitive.
- **Scoreboard team, food, saturation, experience** — survival state nobody puts in a tab list.
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
%player_has_permission:pumpkin-papi:use%
```

An argument may contain letters, digits and `_ . - : /`. A placeholder that needs an argument
and does not get one resolves to an empty string.

## Commands

All commands need the `pumpkin-papi:use` permission, which operators have by default.

| Command | What it does |
|:--|:--|
| `/papi parse <text>` | Resolves `<text>` and prints the result |
| `/papi` | Prints the usage line and every placeholder |
| `/papi expansions` | Lists registered expansions and who owns them |

`/papi parse` reports which placeholders it could not resolve, so it doubles as a way to check a
config. Tab completion suggests placeholder ids.

## Using it from a plugin

Add the crate. It is both the client and the provider, so one dependency covers both.

```toml
[dependencies]
pumpkin-plugin-api = "0.1.0-dev"
pumpkin-papi = "0.1"
```

Consuming placeholders:

```rust
use pumpkin_papi::PapiClient;

let papi = PapiClient::new();

let line = papi.set_placeholders(Some("Steve"), "%player_ping%ms %ranks_prefix%")?;
println!("{}   unresolved: {:?}", line.text, line.unresolved);

let online = papi.get_placeholder_value(None, "server_online")?;   // Option<String>
let all    = papi.get_registered_placeholders()?;
```

Refreshing a tab list or a scoreboard for every player, which is the case batching exists for:

```rust
let lines = [("Steve", "%player_ping%ms"), ("Alex", "%player_ping%ms")];
let results = papi.set_placeholders_batch(&lines)?;   // one message, not one per player
```

Providing placeholders, which is the `PlaceholderExpansion` equivalent:

```rust
use pumpkin_papi::{answer, Cache, IpcMessage, PapiClient, PluginId};

fn on_load() {
    // `Cache::Ttl` lets the provider reuse your values instead of asking every
    // time. The provider clamps it, so a long TTL cannot serve stale ranks.
    PapiClient::new().register_expansion("ranks", &["prefix", "suffix"], Cache::Ttl { ms: 5_000 })?;
}

fn handle_ipc_message(_from: PluginId, message: IpcMessage) -> Result<IpcMessage, String> {
    Ok(answer(&message, |ctx| match ctx.name {
        "prefix" => Some(rank_of(ctx.viewer)),
        _ => None,
    }))
}
```

`ctx` carries `namespace`, `id`, `name`, `viewer` and `argument`. Return `None` to decline, and
the placeholder stays in the text. No JSON, no message routing, no parsing.

List `dependencies = ["PumpkinPAPI"]` in your plugin metadata so you are only loaded once the
provider is ready to answer.

### Caching

An expansion that opts in with `Cache::Ttl` has its values cached per viewer, the way PAPI's
`Cacheable` works. This is worth doing: on a server of any size, a tab list refreshing once a
second asks for the same handful of placeholders for every player, every second, and most of
those values barely move. The provider caps a TTL at 60 seconds so a plugin cannot serve stale
data indefinitely.

### Other languages

The provider does not care what language you write in. The protocol is a set of JSON objects over
Pumpkin's [inter-plugin IPC][ipc], which is part of the public WIT, so every SDK has a binding for
it. What that costs you today:

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
<- {"ok":true,"protocol":0,"name":"PumpkinPAPI","version":"0.1.0",
    "placeholders":17,"expansions":2}
```

### `get_registered_placeholders`

```json
-> {"op":"get_registered_placeholders"}
<- {"ok":true,"placeholders":[
      {"id":"player_name","description":"The viewer's name",
       "namespace":"player","source":"PumpkinPAPI"}
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

`placeholders` may be omitted or empty, which means the expansion answers whatever it is asked
for. A later call replaces the name list and the cache setting. The response echoes back the TTL
the provider actually applied, after clamping.

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

## Development

```bash
cargo test
cargo clippy --all-targets
cargo build --release --target wasm32-wasip2
```

The tests cover the parts that are easy to break quietly: the token scanner, the JSON boundary,
and the expansion table. They need no Minecraft client and no server, so they are the fast
feedback loop. What they cannot check is what a real client sees, which is the one command in
[Install](#install).

### Layout

| Path | What lives there |
|:--|:--|
| `src/protocol.rs` | The wire format, limits, validation |
| `src/tokens.rs` | Scanning and substituting `%placeholder%` |
| `src/expansion.rs` | Expansions and namespaces |
| `src/builtins.rs` | The built in placeholder table |
| `src/client.rs` | The consumer side, and `answer` for expansion authors |
| `src/command.rs` | `/papi` |
| `src/plugin.rs` | The provider's IPC surface and cache |
| `tests/protocol.rs` | Unit tests |

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
  only things consumers depend on. The `python` branch is already one.

If you send a pull request, please say which Pumpkin version you built against and whether you
tested it in game. Bug reports are most useful with the log line the provider printed.

## Credits

- [PlaceholderAPI](https://github.com/PlaceholderAPI/PlaceholderAPI), for the model this follows.
  Nothing is copied from it.
- [RookTAB](https://github.com/xRookieFight/RookTAB), the existing Pumpkin tab list plugin, as a
  reminder that placeholders are worth having.
- Epix Development, for the work that came before under that name.

## License

GPL-3.0-only, matching the Pumpkin server. See [LICENSE](LICENSE).

One consequence worth knowing: this crate is meant to be depended on, and GPL-3.0 carries over to
anything that copies it. Talking to the provider over IPC does not, because no code is shared, so
a permissively licensed plugin can speak the protocol without touching this repository.
