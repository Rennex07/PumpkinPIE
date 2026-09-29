# PumpkinPIE

**P**laceholder **I**ntegration **E**ngine for [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin),
modelled on [PlaceholderAPI](https://github.com/PlaceholderAPI/PlaceholderAPI). No code is copied
from it, and the protocol here is its own thing rather than a port.

It answers `%player_name%`, `%player_ping%`, `%server_online%` and friends, and lets other plugins
add their own placeholders by registering an expansion.

```
%player_ping%ms  %player_name%  %server_online%/%server_max_players%
```

## Documentation

Pick the one you need. Each page stands on its own.

| I want to... | Read |
| --- | --- |
| **Ask** for placeholders in my plugin | [docs/consuming.md](docs/consuming.md) |
| **Provide** placeholders to other plugins | [docs/expansions.md](docs/expansions.md) |
| Implement a client in **another language** | [docs/protocol.md](docs/protocol.md) |
| Know **why it works this way** | [docs/design.md](docs/design.md) |
| **Build, test, contribute** | [CONTRIBUTING.md](CONTRIBUTING.md) |
| See **what was verified, per version** | [CHANGELOG.md](CHANGELOG.md) |

The two plugin pages meet in the middle, and a third party can join either side later without
either of them changing:

```
your plugin  ->  PumpkinPIE  ->  whichever plugin registered the namespace
  "%ranks_prefix%"      "Admin"
```

PumpkinPIE is the middleman. It answers the `player`, `server` and `pie` placeholders itself and
asks the owner of a namespace for anything else, so a tab list, a scoreboard, a ranks plugin and an
economy plugin can each be written by someone who never met.

The short version of the first two, with the parts you fill in yourself marked. Note the table
order, which is load-bearing:

```toml
# Without this your crate builds as a library and produces no component at all,
# so the server silently ignores it. This is the single most common way a
# Pumpkin plugin fails to load.
[lib]
crate-type = ["cdylib"]

[dependencies]
pumpkin-pie = { git = "https://github.com/Rennex07/PumpkinPIE" }

# You need this too: `pumpkin-plugin-api` is not a transitive dependency you can
# use without declaring it, because `impl Plugin` and `register_command` come
# from it. Match the version to the Pumpkin release you build against.
pumpkin-plugin-api = "0.2.0"
tracing = "0.1"

# A patch in this repo's workspace has no effect on yours, so it goes in your
# manifest too. The path is the `../Pumpkin` checkout from Install below. If you
# installed PumpkinPIE from a release you do not have one, so clone Pumpkin.
#
# This must be the LAST table. TOML tables are order-sensitive: any table
# written after this one falls inside it, so a [profile] below would be read as
# a patch entry and fail, or worse, not fail.
[patch.crates-io]
pumpkin-plugin-api = { path = "../Pumpkin/crates/pumpkin-plugin-api" }
```

Build it with the wasm target, and the `.wasm` lands in your server's `plugins/`:

```bash
rustup target add wasm32-wasip2
cargo build --release --target wasm32-wasip2
# -> target/wasm32-wasip2/release/<your-crate-name>.wasm, into plugins/
```

An unsigned `.wasm` logs a warning about untrusted sources. That is advisory — the plugin loads.

```rust
use pumpkin_pie::PieClient;

let pie = PieClient::new();
// Inside `Plugin` methods, the `?` needs spelling out, because the trait's
// `Result` carries a `String` and there is no `From<PieError>` for one.
let line = pie.set_placeholders(Some("Steve"), "%player_ping%ms").map_err(|e| e.to_string())?;
tracing::info!("{}", line.text);
```

**Depend on `pumpkin-pie` only.** Every Pumpkin plugin exports a symbol called `init-plugin`, so
linking both crates fails to compile. See [docs/design.md](docs/design.md#why-two-crates).

Full setup, including what to do if you installed from a release and have no Pumpkin checkout:
[docs/consuming.md](docs/consuming.md#setup).

## Rules worth knowing before anything else

- **A placeholder is `%namespace_name%`.** The namespace is everything before the *first*
  underscore, so `%luckperms_prefix%` belongs to whoever registered `luckperms`. A namespace may
  therefore not contain an underscore.
- **`player`, `server` and `pie` are reserved.** The provider answers them and an expansion cannot
  claim them.
- **Ids are case insensitive.** `%Player_Ping%` and `%player_ping%` are the same placeholder.
- **A placeholder nobody can resolve is left in the text as written.** Not an error, not an empty
  string. This includes a player placeholder resolved with no viewer.
- **There is no escape for `%`.** `%%player_name%%` resolves to `%Steve%`, same as in Java.
- **One call per line, not per placeholder.** Every placeholder the provider cannot answer itself
  costs a round trip, so resolve a whole line at once. A placeholder repeated within one line is
  resolved once.

## Install

### From a release

Download `pumpkin_pie_plugin.wasm` from the
[releases page](https://github.com/Rennex07/PumpkinPIE/releases) and drop it into your Pumpkin
server's `plugins/` folder. That is the whole install. Start the server and you should see:

```
[INFO] PumpkinPIE 0.2.2 ready with 11 built in placeholders
```

**Use a release built against your server.** A component is compiled against a specific
`pumpkin-plugin-api` WIT, and one built against a different one can fail to load with
`type-checking export func \`handle-event\``. Each release says which Pumpkin revision it was built
from in its notes.

### From source

Source builds build against a local Pumpkin checkout, so clone both as siblings:

```bash
git clone https://github.com/Pumpkin-MC/Pumpkin.git
git clone https://github.com/Rennex07/PumpkinPIE.git
```

```
somewhere/
├── Pumpkin/          <- the server sources
└── PumpkinPIE/      <- this
```

If you keep them elsewhere, edit the `[patch.crates-io]` path in `Cargo.toml`. It points at
`../Pumpkin/crates/pumpkin-plugin-api`.

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

| Placeholder | Example | Notes |
| :-- | :-- | :-- |
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

**Player placeholders need a viewer.** With no viewer they are left in the text as written and
listed in `unresolved`, exactly like an unknown placeholder — so `%player_name%` resolved without a
viewer comes back as the literal `%player_name%`, not an empty string. If you need one line to work
both globally and per player, resolve it per player.

The list is deliberately short, and a placeholder earns a place here only when it is **vanilla state
a third party has no better source for**. [docs/design.md](docs/design.md#why-so-few-built-ins)
covers what that leaves out and why, including the two that need a change upstream to add.

### Arguments

A placeholder can take one argument, written after a colon:

```
%player_has_permission:PumpkinPIE:use%
```

An argument may contain letters, digits and `_ . - : /`. The usual way to pass one is by writing it
in the text for `set_placeholders`.

The Rust `get_placeholder_value` cannot: it takes an id and has no argument parameter, so asking it
for `player_has_permission` always reports `known: false`. That is a limitation of the client, not
of the protocol — the wire format carries an `argument` field, which
[docs/protocol.md](docs/protocol.md#get_placeholder_value) documents.

## Commands

All commands need the `PumpkinPIE:use` permission, which is granted to **op level 2 and above** by
default. The node is registered when the plugin loads rather than declared in its metadata, so it
does not show up in a permissions listing.

The node has to be namespaced with the plugin's exact name. `PumpkinPIE:use`, case-sensitive —
`pumpkin-pie:use` is refused, and a node that was never registered denies the command to *everyone*,
operators included.

| Command | What it does |
| :-- | :-- |
| `/pie parse <text>` | Resolves `<text>` and prints the result |
| `/pie` | Prints the usage line and every placeholder |
| `/pie expansions` | Lists registered expansions and who owns them |

`/pie parse` reports what it could not resolve as a **red error line** and returns a non-zero
result, so it doubles as a way to check a config. Tab completion offers `%id%` values, up to 20 at
a time.

## License

GPL-3.0-only, matching the Pumpkin server. See [LICENSE](LICENSE).

One consequence worth knowing: this crate is meant to be depended on, and GPL-3.0 carries over to
anything that copies it. Talking to the provider over IPC does not, because no code is shared, so a
permissively licensed plugin can speak the protocol without touching this repository.
