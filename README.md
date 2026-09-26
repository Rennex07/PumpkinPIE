# PumpkinPAPI

A placeholder provider for [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin), modelled on
[PlaceholderAPI](https://github.com/PlaceholderAPI/PlaceholderAPI).

It answers `%player_name%`, `%server_online%`, `%player_ping%` and friends, and it lets other
plugins add their own placeholders by registering an expansion. Any Pumpkin plugin can consume
it, in any language Pumpkin has an SDK for.

```
%player_name%  %player_ping% ms  %server_online%/%server_max_players%
```

## Status

Early. Protocol version `0`, and the shape of the protocol may still change. Built-in
placeholders cover the vanilla player and server state; world, biome and item placeholders are
not there yet.

## Why

Pumpkin has no PlaceholderAPI equivalent. A tab list plugin, a scoreboard plugin or a chat
plugin that wants `%player_ping%` currently has to reach into the server API itself, and two
plugins that both want to show a rank end up inventing two incompatible formats.

PumpkinPAPI gives them one place to ask. It is a plugin, not a change to Pumpkin, so it can be
installed and removed without touching the server.

## How it works

The provider owns a registry. Plugins that only *consume* placeholders ask it to resolve a
string. Plugins that also *provide* placeholders register a namespace, and the provider calls
them back when one of their placeholders is needed.

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
      |<------------ "42 ms Admin"      |                                |
```

This is the same shape Java PlaceholderAPI uses:

| Java PlaceholderAPI                       | PumpkinPAPI                                    |
|:------------------------------------------|:-----------------------------------------------|
| `PlaceholderAPI.setPlaceholders(p, text)` | `set_placeholders`                             |
| `getPlaceholderValue(p, text)`            | `get_placeholder_value`                        |
| `getRegisteredPlaceholders()`             | `get_registered_placeholders`                  |
| `registerPlaceholderExpansion(exp)`       | `register_expansion`                           |
| `unregisterPlaceholderExpansion(exp)`     | `unregister_expansion`                         |
| `PlaceholderExpansion.getIdentifier()`    | the `namespace` you register                   |
| `onRequest(player, identifier)`           | `on_request` (the provider sends this to you)  |
| the built in `internal` expansion         | the `player`, `server` and `papi` namespaces   |

A few rules come straight from PlaceholderAPI and are worth knowing up front:

- **A placeholder is `%namespace_name%`.** The namespace is everything before the first
  underscore, so `%luckperms_prefix%` belongs to whoever registered `luckperms`.
- **`player`, `server` and `papi` are reserved.** The provider answers them, and an expansion
  cannot claim them.
- **Ids are case insensitive.** `%Player_Ping%` and `%player_ping%` are the same placeholder.
- **A placeholder nobody can resolve is left in the text exactly as written.** It is not
  replaced with an empty string, and it is not an error.
- **There is no escape for `%`.** `%%player_name%%` resolves to `%Steve%`, same as in Java.

## Install

Build the component and drop it in the server's `plugins` directory:

```bash
pip install pumpkin-api-py
pumpkin-plugin-build main -o pumpkin-papi.wasm
```

Copy `pumpkin-papi.wasm` into your Pumpkin server's `plugins/` folder and start the server. You
should see:

```
[INFO] PumpkinPAPI 0.1.0 ready with 21 built in placeholders
```

Then, in game (operator only):

```
/papi parse %player_name% has %player_ping% ms
```

## Built in placeholders

Player placeholders need a viewer. Without one they resolve to an empty string rather than
failing, so the same text works in a global context and in a per player context.

| Placeholder | Example | Notes |
|:--|:--|:--|
| `%player_name%` | `Steve` | |
| `%player_uuid%` | `069a79f4-...` | |
| `%player_ping%` | `42` | Milliseconds |
| `%player_health%` | `6.5` | |
| `%player_max_health%` | `20` | |
| `%player_gamemode%` | `creative` | |
| `%player_x%` | `1.23` | |
| `%player_y%` | `64` | |
| `%player_z%` | `-8.5` | |
| `%player_yaw%` | `90` | |
| `%player_pitch%` | `-12` | |
| `%player_locale%` | `en_us` | The player's client locale |
| `%player_ip%` | `127.0.0.1` | |
| `%player_has_permission:NODE%` | `true` | Takes an argument, see below |
| `%server_online%` | `5` | |
| `%server_max_players%` | `20` | |
| `%server_tps%` | `19.98` | |
| `%server_mspt%` | `4.27` | |
| `%papi_version%` | `0.1.0` | |
| `%papi_protocol%` | `0` | |
| `%papi_expansions%` | `3` | Expansions registered right now |

The list is deliberately short. A placeholder belongs here only when it is vanilla state a
third party has no better source for, which is why there is no `%player_world%`, no
`%time%` family and no `%random_%`. Everything interesting is supposed to arrive as an
expansion.

### Arguments

A placeholder can take an argument, written after a colon:

```
%player_has_permission:pumpkin-papi:use%
```

An argument may contain letters, digits and `_ . - : /`. A placeholder that needs an argument
and does not get one resolves to an empty string.

## Commands

All commands need the `pumpkin-papi:use` permission, which operators have by default.

| Command | What it does |
|:--|:--|
| `/papi parse <text>` | Resolves `<text>` for you and prints the result |
| `/papi` | Prints the usage line and every placeholder |
| `/papi expansions` | Lists the registered expansions and who owns them |

`/papi parse` reports which placeholders it could not resolve, so it doubles as a way to check
a config. Tab completion suggests placeholder ids.

## Using it from a plugin

The protocol is UTF-8 JSON over Pumpkin's [inter-plugin IPC][ipc]. The provider's plugin name,
`PumpkinPAPI`, is its address, so any language with a Pumpkin SDK can talk to it. There is no
client library to install; a call is a few lines.

Set `dependencies` to `["PumpkinPAPI"]` in your plugin metadata, so your plugin is only loaded
once the provider is ready to answer.

[ipc]: https://github.com/Pumpkin-MC/Pumpkin/blob/master/crates/pumpkin-plugin-wit/v0.1/ipc.wit

### One call per line, not per placeholder

This is the one thing worth getting right. Every placeholder that the provider cannot answer
itself costs a round trip into the expansion that owns it, so resolve **a whole line at once**:

```python
# One round trip for the line, however many placeholders it has.
tab_line, _ = papi.set_placeholders(player, "%player_ping%ms %ranks_prefix%")

# Four round trips for the same text.
for placeholder in ("%player_ping%", "%ranks_prefix%", "%server_online%", "%player_name%"):
    papi.get_placeholder_value(placeholder)
```

A repeated placeholder in one call is only resolved once, so `%ranks_prefix% %ranks_prefix%`
still costs a single round trip. A single call is capped at 16 expansion callbacks; past that the
remaining placeholders are left in the text. A tab list refreshing 20 players with one line each
is 20 calls, which is fine.

### Other languages

The provider does not care what language you write in. `ipc` is part of the public WIT, so every
SDK Pumpkin generates has a binding for it, and the only thing language specific is how you
serialise JSON.

What that costs you today:

- **The client is Python only.** `papi/client.py` exists because writing the JSON by hand is
  tedious, not because it is hard. Everyone else gets the snippets below. A Rust or TypeScript
  client module is the single most useful contribution here.
- **The Rust snippet handles two error layers,** because the WIT declares
  `result<result<ipc-message, string>>`. The outer one means the provider was not reachable, the
  inner one means it refused. It is boilerplate, but it is boilerplate.
- **Writing an expansion is more work than consuming one.** You have to route `on_request`
  messages yourself and parse the incoming JSON. In Python that is one `handle_ipc_message` that
  delegates to the client; elsewhere it is a few dozen lines.
- **The viewer is a name or uuid, not a player handle.** That is deliberate, so you do not need
  any WIT specific type to call the provider. The provider does the lookup.
- **Nothing has a timeout.** A call is synchronous, so an expansion that hangs holds up the
  plugin that called it. Keep expansions cheap.


### Python

Copy the `papi/` directory into your plugin, or install it from this repository. It gives you
`PapiClient` and keeps you from hand writing the JSON.

```python
from papi.client import PapiClient

papi = PapiClient()

text, unresolved = papi.set_placeholders(player, "%player_name% %player_ping%ms")
value = papi.get_placeholder_value("server_online")
```

### Rust

`send_ipc_message` returns the WIT's nested result: the outer one says whether the host could
deliver the message at all, the inner one is the provider's answer.

```rust
use pumpkin_api::ipc;

let request = br#"{"op":"set_placeholders","text":"%player_name%","viewer":"Steve"}"#;

// outer Err: the provider is not loaded, or the name is wrong
// inner Err: the provider answered with an error
let Ok(reply) = ipc::send_ipc_message("PumpkinPAPI", request.to_vec()).await else {
    return Ok(());
};
let Ok(reply) = reply else {
    return Ok(());
};
let payload: serde_json::Value = serde_json::from_slice(&reply)?;
let text = payload["text"].as_str().unwrap_or_default();
```

### TypeScript

```ts
import { ipc } from "pumpkin-api";

const request = new TextEncoder().encode(
  JSON.stringify({ op: "set_placeholders", text: "%player_name%", viewer: "Steve" }),
);
const reply = await ipc.sendIpcMessage("PumpkinPAPI", request);
const payload = JSON.parse(new TextDecoder().decode(reply));
```

## Writing an expansion

An expansion is a plugin that answers for its own namespace. This is the `PlaceholderExpansion`
equivalent.

```python
from pumpkin_api import Plugin, context, logging, metadata, register_plugin

from papi import protocol
from papi.client import PapiClient

RANKS = {"Steve": "Admin"}


class RanksPlugin(Plugin):
    def __init__(self):
        super().__init__()
        self.papi = PapiClient()

    def metadata(self) -> metadata.PluginMetadata:
        return metadata.PluginMetadata(
            name="Ranks",
            version="1.0.0",
            authors=["you"],
            description="Ranks as placeholders.",
            dependencies=[protocol.PROVIDER],
            permissions=[],
        )

    def on_load(self, ctx: context.Context) -> None:
        self.papi.on_request(self.rank)
        self.papi.register_expansion("ranks", ["prefix", "suffix"])
        logging.log(logging.Level.INFO, "Ranks registered with PumpkinPAPI")

    def rank(self, namespace, name, viewer, argument):
        if name == "prefix":
            return RANKS.get(viewer, "Member")
        return ""

    def handle_ipc_message(self, sender: str, message: bytes) -> bytes:
        return self.papi.handle_ipc_message(sender, message)


register_plugin(RanksPlugin)
```

Now `%ranks_prefix%` resolves anywhere, and `/papi expansions` lists it.

The rules an expansion has to live by:

- **Return `None` or an empty string to decline.** The placeholder is then left in the text.
- **Do not call `set_placeholders` from inside `on_request` for the same text.** A single
  `set_placeholders` call gets at most 16 callbacks, and anything past that is left in the
  text. That budget exists so a mistake cannot loop forever.
- **The value you return is used verbatim.** The provider does not scan it for placeholders, so
  you cannot accidentally build a chain.
- **Register a namespace you own.** `player`, `server` and `papi` are refused.

## Protocol

Every message is a UTF-8 JSON object, at most 64 KiB. A request carries an `op`; a response
carries `ok`, and `error` when `ok` is false. Anything malformed gets an error response rather
than an exception.

### `ping`

```json
-> {"op": "ping"}
<- {"ok": true, "protocol": 0, "name": "PumpkinPAPI", "version": "0.1.0",
    "placeholders": 21, "expansions": 2}
```

### `get_registered_placeholders`

```json
-> {"op": "get_registered_placeholders"}
<- {"ok": true, "placeholders": [
      {"id": "player_name", "description": "Name of the player the text is rendered for",
       "namespace": "papi", "source": "PumpkinPAPI"}
    ]}
```

### `set_placeholders`

`viewer` is a player name or uuid, or omitted. `argument` is only useful for a single
placeholder that takes one.

```json
-> {"op": "set_placeholders", "text": "%player_name%", "viewer": "Steve"}
<- {"ok": true, "text": "Steve", "unresolved": []}
```

### `get_placeholder_value`

```json
-> {"op": "get_placeholder_value", "id": "server_online", "viewer": "Steve"}
<- {"ok": true, "known": true, "value": "1"}
```

### `register_expansion`

`placeholders` may be omitted or empty, which means the expansion answers whatever it is asked
for. A later call replaces the name list. Re-registering the same namespace from a different
plugin takes it over.

```json
-> {"op": "register_expansion", "namespace": "ranks", "placeholders": ["prefix"]}
<- {"ok": true, "namespace": "ranks", "placeholders": ["prefix"]}
```

### `unregister_expansion`

Drops every namespace the calling plugin registered.

```json
-> {"op": "unregister_expansion"}
<- {"ok": true, "namespaces": ["ranks"]}
```

### `on_request`

Sent by the provider to the plugin that owns the namespace, once per placeholder it cannot
answer itself. The `id` is the full placeholder, `name` is the part after the namespace.

```json
-> {"op": "on_request", "namespace": "ranks", "id": "ranks_prefix", "name": "prefix",
    "viewer": "Steve", "argument": null}
<- {"ok": true, "value": "Admin"}
```

## Differences from Java PlaceholderAPI

Worth reading before you port a config over:

- **Placeholder ids may not contain `-` or `.`.** Only letters, digits and `_`. A Java
  expansion using `%server_online-players%` needs renaming.
- **A callback is an IPC message, not a function call.** An expansion that is not loaded, or
  that answers with an error, leaves its placeholder in the text instead of raising.
- **No `%time%`, `%world_*%` or random placeholders yet.** They need world and time getters that
  the plugin WIT does not expose cheaply.
- **No offline player support.** The viewer has to be online, or resolvable by name or uuid.
- **`getPlaceholderValue` takes an id, not a text.** There is no `%placeholder%` string to parse.

## Development

```bash
pip install -r requirements.txt

python -m unittest discover -s tests -t .    # 74 tests, no server needed
pumpkin-plugin-build main -o pumpkin-papi.wasm
```

The tests cover the parts that are easy to break quietly: token scanning, the JSON boundary, the
registry, and the provider's IPC surface against a faked host. They need no Minecraft client and
no server, so they are the fast feedback loop. The only thing they cannot check is what a real
client sees, which is the one command in [Install](#install).

### Layout

| Path | What lives there |
|:--|:--|
| `main.py` | The provider plugin: commands, IPC surface, resolution |
| `papi/tokens.py` | Scanning and substituting `%placeholder%` |
| `papi/protocol.py` | The wire format, limits, validation |
| `papi/registry.py` | Expansions and namespaces |
| `papi/defaults.py` | The built in placeholder table |
| `papi/client.py` | The consumer side, for Python plugins |
| `tests/` | Unit tests, plus fakes for the host objects |

`papi/` deliberately has no Pumpkin imports outside `client.py`, so the logic can be tested on
plain CPython.

## Contributing

**Pull requests are welcome.** So are issues, and so is telling me that a placeholder you need
is missing.

Some things that would help:

- **A placeholder you actually want.** `%player_world%` and `%player_display_name%` are the most
  requested ones. They are missing because the getters hand back a host resource and there is no
  way to release it, so adding them properly means a change to
  [the WIT](https://github.com/Pumpkin-MC/Pumpkin/tree/master/crates/pumpkin-plugin-wit), not to
  this plugin.
- **Client libraries.** `papi/client.py` is Python only. A Rust or TypeScript client module
  would make the protocol easier to adopt, and the protocol is small enough to fit in a language
  binding.
- **Tests.** Anything that turns bytes into values or back, and anything that is easy to break
  without noticing.
- **A second implementation.** The protocol is the contract, not this plugin. If you would
  rather write the provider in Rust, the protocol version and the operation names are the only
  things consumers depend on.

If you send a pull request, please say which Pumpkin version you built against and whether you
tested it in game. Bug reports are most useful with the log line the provider printed.

## Credits

- [PlaceholderAPI](https://github.com/PlaceholderAPI/PlaceholderAPI), for the model this follows.
  Nothing is copied from it; it is GPL and this is not.
- [RookTAB](https://github.com/xRookieFight/RookTAB), the existing Pumpkin tab list plugin, as a
  reminder that placeholders are worth having.
- Epix Development, for the work that came before under that name.

## License

GPL-3.0-only, matching the Pumpkin server. See [LICENSE](LICENSE).

One consequence worth knowing: `papi/client.py` is meant to be copied into other plugins, and
GPL-3.0 carries over to anything that copies it. A plugin that vendors the client has to be
GPL-3.0 too. Talking to the provider over IPC does not, because no code is shared, so a
permissively licensed plugin can use the protocol without touching this repository. If you would
rather the client be MIT or Apache-2.0 so that plugins can vendor it without inheriting the
copyleft, that is a one file change and a split license, and it is your call.
