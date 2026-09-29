# Consuming placeholders

For a plugin that *asks* for placeholders. A tab list, a scoreboard or a chat formatter is usually
such a plugin, and so is anything else that renders a string someone else owns. If you want to
*provide* placeholders instead, see [expansions.md](expansions.md).

**You are one of three parties, and only one of them is PumpkinPIE.** It answers built-ins itself
and asks whichever plugin registered a namespace to answer for its own placeholders. Nothing is
wired to your plugin in particular, and nothing has to know you exist.

```
your plugin  ->  PumpkinPIE  ->  the plugin that registered the namespace
   "%ranks_prefix%"     "Admin"
```

So this page covers asking. What you do with the answer is your business: a scoreboard plugin calls
`set_placeholders` and then hands the string to Pumpkin's own
[`scoreboard.wit`](https://github.com/Pumpkin-MC/Pumpkin/blob/master/crates/pumpkin-plugin-wit/v0.1/scoreboard.wit)
(`add-objective`, `set-display-slot`, `add-score`), a chat plugin formats it, a tab list sends it
in a header. All of that is Pumpkin's client-facing API, not this crate's, and none of it is
covered here.

## Setup

Your crate needs all of this, not just the first line — a plugin missing any of it either fails to
compile or, worse, builds a library the server silently ignores. Alongside your own `[package]`
and `[lib]` tables:

```toml
[lib]
# Without this you get an rlib, no .wasm, and a plugin folder the server skips
# with nothing in the log. This is the most common way a plugin fails to load.
crate-type = ["cdylib"]

[dependencies]
pumpkin-pie = { git = "https://github.com/Rennex07/PumpkinPIE" }
# Not transitive: `impl Plugin`, `Context` and `register_command` come from here,
# so a plugin cannot use them without declaring it. Match your Pumpkin release.
pumpkin-plugin-api = "0.2.0"
tracing = "0.1"

# Keep [patch.crates-io] last - TOML tables are order-sensitive, and a dependency
# written below it becomes a patch entry rather than a dependency.
[patch.crates-io]
pumpkin-plugin-api = { path = "../Pumpkin/crates/pumpkin-plugin-api" }
```

Then build it, and drop the artifact in your server's `plugins/`:

```bash
rustup target add wasm32-wasip2
cargo build --release --target wasm32-wasip2
# -> target/wasm32-wasip2/release/<your-crate-name>.wasm
```

Two things about that.

**The `[patch.crates-io]` must be in *your* `Cargo.toml`.** A patch declared in this repository's
workspace has no effect on yours. The path is the `../Pumpkin` checkout from
[Install](../README.md#from-source); adjust it to wherever yours lives. If you installed PumpkinPIE
from a release you do not have one, so clone Pumpkin.

**Do not also depend on `pumpkin-pie-plugin`.** Every Pumpkin plugin exports a symbol called
`init-plugin`, so linking both crates fails to compile with `duplicate symbol: init-plugin`. The
reason there is no workaround worth having is in
[design.md](design.md#why-two-crates).

## The API

Inside a `Plugin` method — which is where this code ends up — the `?` needs spelling out. The
trait's `Result` carries a `String` and there is no `From<PieError>` for one:

```rust
use pumpkin_pie::PieClient;
use pumpkin_plugin_api::{Context, Plugin, PluginMetadata, Result};
use tracing::info;

fn on_load(&self, _context: Context) -> Result<()> {
    let pie = PieClient::new();
    let line = pie
        .set_placeholders(Some("Steve"), "%player_ping%ms %ranks_prefix%")
        .map_err(|e| e.to_string())?;              // <-- not a bare `?`
    info!("{}   unresolved: {:?}", line.text, line.unresolved);
    Ok(())
}
```

The same calls from a free function, where `?` works normally:

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

`crates/pumpkin-pie-consumer` is a complete working consumer, including the `Plugin` impl and the
`register_plugin!` call, if you would rather read a whole one.

`PieClient` holds no state, so keep one around for the life of your plugin.

| Call | Gives you |
| :-- | :-- |
| `set_placeholders(viewer, text)` | One line resolved, plus the ids that stayed in it |
| `set_placeholders_batch(lines)` | The same, for many players in one message |
| `get_placeholder_value(viewer, id)` | One value, or `None` if it is not registered |
| `get_registered_placeholders()` | Every id the provider knows, built in and contributed |

The `viewer` is a player **name**, not a handle, and it has to be online — the provider looks
players up by name only, so a uuid resolves nothing. The name is also what the provider hands an
expansion, which is all it ever gets to work with.

If your provider is registered under a different name than `PumpkinPIE`, use
`PieClient::with_provider("...")` rather than the default `PieClient::new()`.

Its `/piecheck` command runs each call and logs what came back.

## Refreshing a tab list or scoreboard

This is what batching exists for. One message per refresh instead of one per player:

```rust
// (viewer, text) - the same order as set_placeholders' two arguments.
let lines = [("Steve", "%player_ping%ms"), ("Alex", "%player_ping%ms")];
let results = pie.set_placeholders_batch(&lines)?;
```

Results come back in the order you sent them, so `results[i]` is line `i`. The array can also come
back **shorter** than the requests: a line over the 32 KiB limit is dropped silently. Check the
lengths match before zipping results back to players.

Every batch line carries a viewer. The protocol allows a line with no viewer, but there is no Rust
API for it yet, so a batch is always per player — which means **a server-wide line like
`%server_online% online` cannot go in a batch.** Send it as its own `set_placeholders(None, ...)` on
each refresh.

## Limits

These bound you as a consumer, not just an expansion author:

| Limit | Value | Exceeding it |
| :-- | :-- | :-- |
| Message size, either direction | 1 MiB | the message is refused |
| One line of text | 32 KiB | the request is refused |
| Lines per `set_placeholders_batch` | 8192 | the whole batch is refused |
| Expansion callbacks per line | 16 | the rest are **left in the text**, with a warning in the log |

That last row is the one that will surprise you. Sixteen placeholders from other plugins on a single
line is easy to reach on a decorated tab list, and nothing in the response tells you it happened —
you just get literal `%...%` back. If a line is elaborate, this is a likely cause.

## Things that will surprise you

**A placeholder nobody can resolve stays in the text.** It is not an error and not an empty string,
so `%nope_nope%` comes back as `%nope_nope%` and is listed in `line.unresolved`. Check that list if
a config is not rendering — a leftover `%` is the symptom of an unknown or unloaded namespace.

**A player placeholder with no viewer is also left in the text.** This is the single most common
surprise. `set_placeholders(None, "%player_name%")` gives you back the literal `%player_name%`, not
`""`, because the provider cannot tell a viewerless player placeholder from an unknown one. If a
line has to work both globally and per player, resolve it per player.

**Those two are indistinguishable from the text alone.** Both land in `unresolved` and nothing says
which happened. To tell them apart, ask directly:

```rust
// Known, but your line had no viewer:
pie.get_placeholder_value(Some("Steve"), "player_ping")?;   // Some("42")
// Genuinely not registered:
pie.get_placeholder_value(Some("Steve"), "nope_nope")?;      // None
```

So: an id in `unresolved` that `get_placeholder_value` *does* return means you lost the viewer. An
id it also returns `None` for means nobody provides it — usually a plugin that is not loaded.

**`get_placeholder_value` returns `None` for three different things** — not registered, viewer not
online, and a placeholder that needs an argument (it has no argument parameter). It is a probe, not
an error channel.

**`unresolved` is a diagnostic, not a failure.** It is empty on success and populated on a partial
resolve; neither case is an `Err`.

## Debugging a broken config

In game, as an operator of level 2 or above:

```
/pie parse %player_ping%ms  %ranks_prefix%
```

It resolves the text and prints what it could **not** resolve as a red error line, returning a
non-zero command result. That makes it the fastest way to find a typo without a client connected.
`/pie` alone lists every registered placeholder, and `/pie expansions` shows who owns each namespace
— useful when a namespace you expect is simply not loaded.

## Errors

Every call returns `Result<_, PieError>`. `PieError::Unreachable` is the one to expect on a first
run: the provider plugin is not installed, is named differently, or your plugin messaged itself.
Pumpkin reports all three the same way, so check the server log first.

| Variant | Means |
| :-- | :-- |
| `Unreachable` | No provider answered. Check it is installed and named `PumpkinPIE`. |
| `Refused` | The provider answered and said no. The message says why. |
| `Malformed` | The reply was not something this client can read. |
| `Mismatched` | The reply was for a different operation. A protocol bug. |
| `Unencodable` | The request was too big to send, so it never left. |

## Comparing with Java PlaceholderAPI

| Java PlaceholderAPI | PumpkinPIE |
| :-- | :-- |
| `PlaceholderAPI.setPlaceholders(p, t)` | `set_placeholders` |
| `getPlaceholderValue(p, text)` | `get_placeholder_value` (takes an **id**, not text) |
| `getRegisteredPlaceholders()` | `get_registered_placeholders` |
| `setPlaceholders` for many players | `set_placeholders_batch` |
| the built in `internal` expansion | the `player`, `server` and `pie` namespaces |

Worth reading before porting a config over:

- **`get_placeholder_value` takes an id, not a text.** There is no `%placeholder%` string to parse,
  so it cannot handle a placeholder with an argument.
- **Placeholder ids may not contain `-` or `.`** — only letters, digits and `_`. A config using
  `%server_online-players%` needs renaming.
- **No offline player support.** The viewer has to be online, and the built ins need a real
  `Player`, not just a name that matches nothing.
- **Nothing has a timeout.** A call is synchronous, so a slow provider holds up whoever called it.
