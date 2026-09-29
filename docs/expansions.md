# Providing placeholders

For a plugin that *supplies* placeholders to others — ranks, prefixes, economy, a custom stat. If
you only want to show placeholders, see [consuming.md](consuming.md).

This is the `PlaceholderExpansion` equivalent. It is a full plugin: `impl Plugin`, a
`PluginMetadata`, and `register_plugin!`.

Three rules to pick a namespace by: it may not contain an underscore, it may not begin with a
digit, and it may not be `player`, `server` or `pie` — the provider answers those itself and will
refuse to hand them over. Breaking any of them is an `Err` from `register_expansion`, not a silent
no-op.

## A complete expansion

```rust
// `IpcMessage` and `PluginId` come from `pumpkin_pie`, not from
// `pumpkin_plugin_api`: the API crate keeps its `wit` module private, so the
// IPC types are not re-exported from there. `pumpkin_pie` re-exports them so
// you need only two imports.
use pumpkin_pie::{answer, Cache, IpcMessage, PieClient, PieError, PluginId};
use pumpkin_plugin_api::{Context, Plugin, PluginMetadata, Result};

// `register_plugin!` is a macro from `pumpkin_plugin_api`, and has to be
// imported like one. It is gated to wasm because a host build has no use for the
// `init-plugin` export it defines, and defining it there collides with the
// copy in `pumpkin-pie` when both land in one binary.
#[cfg(target_arch = "wasm32")]
use pumpkin_plugin_api::register_plugin;

// Logging is `tracing`, not `log`. Pumpkin's API surface uses it throughout.
use tracing::info;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

// Your actual data. `RwLock` because `handle_ipc_message` only gets `&self`,
// so anything you write to later needs interior mutability. A real plugin would
// fill this in `on_load`; see "Where state lives" for why it is a `static` and
// not a field on the plugin.
static RANKS: LazyLock<RwLock<HashMap<String, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

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
            // The provider's exact name, and it must be listed or you can load
            // before it is ready to answer.
            dependencies: vec!["PumpkinPIE".to_string()],
            permissions: vec![],
        }
    }

    fn on_load(&self, _context: Context) -> Result<()> {
        // `on_load` returns `Result<(), String>` and there is no
        // `From<PieError> for String`, so the `?` needs the conversion spelled
        // out. This is the one line most likely to fail on your first build.
        self.register().map_err(|error| error.to_string())?;
        Ok(())
    }

    fn on_unload(&self, _context: Context) -> Result<()> {
        // Deliberately does NOT call `unregister_expansion`. See "Cleanup" below.
        Ok(())
    }

    fn handle_ipc_message(
        &self,
        _from: PluginId,
        message: IpcMessage,
    ) -> std::result::Result<IpcMessage, String> {
        // `std::result::Result`, not `Result`: `pumpkin_plugin_api` exports its
        // own `Result` alias, and the one in scope here is theirs.
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
        info!("ranks registered: {:?}", applied.cache);
        Ok(())
    }
}

fn rank_of(viewer: Option<&str>) -> Option<String> {
    let viewer = viewer?;
    let ranks = RANKS.read().ok()?;
    Some(ranks.get(viewer).cloned().unwrap_or_else(|| "Member".to_string()))
}

#[cfg(target_arch = "wasm32")]
register_plugin!(Ranks);
```

The two `#[cfg(target_arch = "wasm32")]` gates are not optional decoration. `register_plugin!`
defines the `init-plugin` export that every Pumpkin plugin needs, and a host build has no use for
it. Dropping the gate compiles fine and then fails on the server with a component that exports
nothing.

`crates/pumpkin-pie-testexp` is a working expansion you can copy from.

## The four things that are easy to get wrong

- `handle_ipc_message` returns `std::result::Result`, not `Result`. `pumpkin_plugin_api` exports
  its own `Result` alias, so with it in scope the prelude's `Result` is shadowed and the trait
  signature needs the fully qualified name.
- `answer` returns `Result<Vec<u8>, ProtocolError>`, so the `map_err` is what turns it into the
  `String`-errored result the trait wants.
- `register_plugin!` is a macro from `pumpkin_plugin_api`, not from this crate.
- A namespace may not contain an `_`. The namespace of `%a_b%` is everything before the **first**
  underscore, so a namespace containing one could never be reached — and it would not look
  unreachable, because `%my_plugin_rank%` would quietly be forwarded to whoever owns `my`.

## Where state lives

This is the first thing that trips people up, so it is worth being blunt: **you cannot put your data
on the plugin value.**

```rust
fn handle_ipc_message(&self, ...) -> ...   // &self, not &mut self
fn new() -> Self                           // takes no arguments
```

`&self` cannot be mutated, and `new()` gives you nowhere to put a balance table. So a struct field
like `ranks: HashMap<String, String>` does not work, and neither does building one in `on_load` —
the `Context` you get there is dropped when `on_load` returns.

Use a `static` instead, with interior mutability:

```rust
use std::sync::LazyLock;
use std::sync::RwLock;

static RANKS: LazyLock<RwLock<HashMap<String, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

// read, in the resolve closure
let rank = RANKS.read().ok()?.get(viewer).cloned();

// write, whenever the data changes
RANKS.write().ok()?.insert(name, rank);
```

`crates/pumpkin-pie-testexp` uses the same shape with a `static AtomicU64`. An atomic is enough for
a counter; anything you look up by player wants a lock.

**A value changed at runtime stays stale until the TTL expires**, because there is no push API and
no invalidate call. Re-registering *does* clear the cache for your namespaces — the provider drops
everything cached for your plugin on every `register_expansion` — so you can force a refresh that
way, at the cost of one extra round trip and a re-sent name list. Otherwise pick a TTL that suits
how stale the value may be, or use `Cache::Never` and accept a callback per resolve.

Note that the example above stores a plain `HashMap` and only ever reads it. The moment you write to
it, it needs the lock the next section describes.

## The request context

`ctx` is borrowed data only, and `viewer` is a player **name**, not a handle:

| Field | Type | |
| :-- | :-- | :-- |
| `namespace` | `&str` | lowercased |
| `id` | `&str` | the full id as written in the text, so its case is preserved |
| `name` | `&str` | the part after the first `_`, **lowercased** |
| `viewer` | `Option<&str>` | player name, not a `Player` |
| `argument` | `Option<&str>` | the text after the first `:` |

`name` is lowercased to match how names are registered, so `%ranks_PREFIX%` arrives as `prefix` and
a `match` arm for the name you registered will hit. `id` keeps the case it was written with, which
is what you want for echoing a placeholder back to a player.

If you want to write the resolve function separately, its parameter type is
`RequestContext<'_>`, re-exported from `pumpkin_pie`. That also makes it testable, because `answer`
takes any `FnOnce(RequestContext<'_>) -> Option<String>` — a named function as well as a closure.

## Testing your resolve function

An expansion's logic is a pure function from a request to an `Option<String>`, and you can test it on
the host without a server, a client, or the wasm target. Construct the request, hand the bytes to
`answer`, and read the reply back:

```rust
use pumpkin_pie::protocol::{decode_response, encode};
use pumpkin_pie::{Request, RequestContext, Success, answer};

// A named function, which `answer` accepts just as readily as a closure.
fn resolve(ctx: RequestContext<'_>) -> Option<String> {
    match ctx.name {
        "prefix" => Some(ctx.viewer?.to_string()),
        _ => None,
    }
}

fn ask(namespace: &str, name: &str) -> Option<String> {
    let request = Request::OnRequest {
        namespace: namespace.to_string(),
        id: format!("{namespace}_{name}"),
        name: name.to_string(),
        viewer: Some("Steve".to_string()),
        argument: None,
    };
    let reply = answer(&encode(&request).unwrap(), resolve).unwrap();
    match decode_response(&reply).unwrap().into_success().unwrap() {
        Success::OnRequest { value } => value,
        other => panic!("expected an on_request reply, got {other:?}"),
    }
}

#[test]
fn a_known_name_answers() {
    assert_eq!(ask("ranks", "prefix").as_deref(), Some("Steve"));
}

#[test]
fn an_unknown_name_declines() {
    assert_eq!(ask("ranks", "nonsense"), None);
}
```

Two things to know before you write tests here.

**`on_load` never runs under `cargo test`**, so anything it does to set up your state has not
happened. Put that setup in its own `fn` and call it from both `on_load` and the test, rather than
relying on a `static` it leaves half-initialised.

**The `#[cfg(target_arch = "wasm32")]` gate on `register_plugin!` is what makes this work.** The
gate keeps the `init-plugin` export out of host builds, so a `cdylib` crate with a `#[cfg(test)]`
module compiles and runs on the host with no symbol collision. Do not drop the gate and you lose
both this and a working component.

## Do not call back into the provider

**Never reach for a `PieClient` inside the `resolve` closure.** The provider asks you
synchronously, from inside its own IPC handler, so calling back re-enters it while it is still
inside the call that invoked you, and the server deadlocks.

The same applies to messaging **any other plugin** from inside the closure. It runs on the caller's
thread inside their critical path, so a slow closure stalls every consumer on the server. Keep it to
a lookup and a return.

There is also no `Server` or `Player` reachable from inside the closure — `viewer` is a name and
nothing more. Read what you need from your own state.

Return `None` to decline. The placeholder is then left in the text, exactly as if it were unknown.

## Caching

An expansion that opts in with `Cache::Ttl` has its values cached per viewer, the way Java's
`Cacheable` works. This is worth doing: on a server of any size, a tab list refreshing once a
second asks for the same handful of placeholders for every player, every second, and most of those
values barely move. `Cache::Never`, the default, asks you every time.

The provider caps a TTL at 60 seconds, so a plugin cannot serve stale data indefinitely. Read
`Registered::cache` to see what it actually applied.

`register_expansion` returns a `Registered` with two fields — `placeholders`, the names the provider
now advertises for your namespace, lowercased, and `cache`, the setting it actually applied after
clamping. There is no `namespace` field; the namespace is what you passed in.

## Your name list is not a filter

`placeholders` is what the provider advertises for your namespace in `get_registered_placeholders`
and `/pie`. It is **not** a filter: the provider forwards any `%ranks_*%` to you whether you listed
it or not. That is why the `_ => None` arm in the example above matters. Omit the list, or leave it
empty, if you would rather answer whatever you are asked.

A later `register_expansion` replaces the name list and the cache setting. Names are lowercased on
registration, so `register_expansion("Ranks", &["Prefix"])` works.

## Limits

| Limit | Value | Exceeding it |
| :-- | :-- | :-- |
| Message size, either direction | 1 MiB | the message is refused (`too large`) |
| One line of text | 32 KiB | refused in a single request; **silently dropped** in a batch |
| Lines per `set_placeholders_batch` | 8192 | the whole batch is refused |
| Expansion callbacks per line | 16 | the rest are left in the text, with a warning in the log |
| Cache TTL an expansion may ask for | 60 s | clamped to 60 s |
| Namespace, name, viewer, argument | 128 characters | rejected by the **Rust client** before sending |

Two of those are worth reading twice.

**The callback budget is per line** rather than per message, so a large batch is not penalised for
being large. It counts each *distinct* placeholder in a line once, so a line that repeats a value
still costs one round trip.

**The 128-character limit on a namespace, name, viewer or argument is enforced by `PieClient`, not
by the provider.** The provider validates the length of an id and of a line's text, and nothing
else — it will happily accept a 500-character namespace. The Rust client truncates all four before
sending, so a `PieError::Unencodable` there never reaches the wire. A client in another language has
no such guard.

## A namespace collision is silent

If two plugins register the same namespace, the **last one to load wins**, with no warning. The
provider overwrites the first plugin's entry, so its names disappear from `/pie` and from
`get_registered_placeholders`, and its `on_request` callbacks stop being made — silently, from the
first plugin's point of view.

`register_expansion` does not fail on a collision, and there is no way to query whether a namespace
is already taken. So pick something unlikely to be claimed by another plugin: `ranks` is a
reasonable guess at a prefix and a poor choice for a namespace.

## Differences from Java PlaceholderAPI

- **A callback is an IPC message, not a function call.** An expansion that is not loaded, or that
  answers with an error, leaves its placeholder in the text instead of raising.
- **Placeholder ids may not contain `-` or `.`** — only letters, digits and `_`.
- **No `%time%`, `%world_*%` or `%random_%`.** They need world and time getters the plugin WIT does
  not expose cheaply.

## Cleanup

**Do not call `unregister_expansion` from `on_unload`.** Plugins unload in load order, so by the
time your `on_unload` runs the provider has usually gone already, and messaging a plugin that has
stopped traps the guest: the log shows `Wasm plugin store driver stopped` with no line from your
plugin at all, and the server dies on shutdown with nothing pointing at the cause.

Nothing needs cleaning up regardless. When the provider unloads it discards the whole registry,
namespaces and cached values together, and it drops a source's entries on a normal
`unregister_expansion` if one ever arrives while it is still running.

So leave `on_unload` empty:

```rust
fn on_unload(&self, _context: Context) -> Result<()> {
    Ok(())
}
```

`unregister_expansion` is still useful at *runtime* — if your plugin gives up a namespace while
still loaded, that is the right call. It only drops the calling plugin's own namespaces, so one
plugin cannot unregister another's.
