# Consuming placeholders

For a plugin that shows placeholders — a tab list, a scoreboard, a chat formatter. If you want to
*provide* placeholders instead, see [expansions.md](expansions.md).

## Setup

Add the client crate, which is `pumpkin-pie` — the plain `rlib`, not `pumpkin-pie-plugin`:

```toml
[dependencies]
pumpkin-pie = { git = "https://github.com/Rennex07/PumpkinPIE" }

[patch.crates-io]
pumpkin-plugin-api = { path = "../Pumpkin/crates/pumpkin-plugin-api" }
```

Two things about that snippet.

**The `[patch.crates-io]` must be in *your* `Cargo.toml`.** A patch declared in this repository's
workspace has no effect on yours. The path is the `../Pumpkin` checkout from
[Install](../README.md#from-source); adjust it to wherever yours lives.

**Do not also depend on `pumpkin-pie-plugin`.** Every Pumpkin plugin exports a symbol called
`init-plugin`, so linking both crates fails to compile with `duplicate symbol: init-plugin`. The
reason there is no workaround worth having is in
[design.md](design.md#why-two-crates).

## The API

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

`PieClient` holds no state, so keep one around for the life of your plugin.

| Call | Gives you |
| :-- | :-- |
| `set_placeholders(viewer, text)` | One line resolved, plus the ids that stayed in it |
| `set_placeholders_batch(lines)` | The same, for many players in one message |
| `get_placeholder_value(viewer, id)` | One value, or `None` if the id is not registered |
| `get_registered_placeholders()` | Every id the provider knows, built in and contributed |

`crates/pumpkin-pie-consumer` is a working plugin that does all of this, so you can read a real one
rather than a fragment. Its `/piecheck` command runs each call and logs what came back.

## Refreshing a tab list or scoreboard

This is what batching exists for. One message per refresh instead of one per player:

```rust
let lines = [("Steve", "%player_ping%ms"), ("Alex", "%player_ping%ms")];
let results = pie.set_placeholders_batch(&lines)?;
```

Every batch line carries a viewer. The protocol allows a line with no viewer, but there is no Rust
API for it yet, so a batch is always per player.

## Things that will surprise you

**A placeholder nobody can resolve stays in the text.** It is not an error and not an empty string,
so `%nope_nope%` comes back as `%nope_nope%` and is listed in `line.unresolved`. Check that list if
a config is not rendering — a leftover `%` is the symptom of an unknown or unloaded namespace.

**A player placeholder with no viewer is also left in the text.** This is the single most common
surprise. `set_placeholders(None, "%player_name%")` gives you back the literal `%player_name%`, not
`""`, because the provider cannot tell a viewerless player placeholder from an unknown one. If a
line has to work both globally and per player, resolve it per player.

**`unresolved` is a diagnostic, not a failure.** It is empty on success and populated on a partial
resolve; neither case is an `Err`.

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
