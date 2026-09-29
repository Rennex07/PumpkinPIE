# Providing placeholders

For a plugin that *supplies* placeholders to others — ranks, prefixes, economy, a custom stat. If
you only want to show placeholders, see [consuming.md](consuming.md).

This is the `PlaceholderExpansion` equivalent. It is a full plugin: `impl Plugin`, a
`PluginMetadata`, and `register_plugin!`.

## A complete expansion

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

List `dependencies = ["PumpkinPIE"]` in your metadata so you are only loaded once the provider is
ready to answer.

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

## Do not call back into the provider

**Never reach for a `PieClient` inside the `resolve` closure.** The provider asks you
synchronously, from inside its own IPC handler, so calling back re-enters it while it is still
inside the call that invoked you, and the server deadlocks.

There is also no `Server` or `Player` reachable from inside the closure — `viewer` is a name and
nothing more. Capture whatever else you need when you build it.

Return `None` to decline. The placeholder is then left in the text, exactly as if it were unknown.

## Caching

An expansion that opts in with `Cache::Ttl` has its values cached per viewer, the way Java's
`Cacheable` works. This is worth doing: on a server of any size, a tab list refreshing once a
second asks for the same handful of placeholders for every player, every second, and most of those
values barely move. `Cache::Never`, the default, asks you every time.

The provider caps a TTL at 60 seconds, so a plugin cannot serve stale data indefinitely. Read
`Registered::cache` to see what it actually applied.

## Your name list is not a filter

`placeholders` is what the provider advertises for your namespace in `get_registered_placeholders`
and `/pie`. It is **not** a filter: the provider forwards any `%ranks_*%` to you whether you listed
it or not. That is why the `_ => None` arm in the example above matters. Omit the list, or leave it
empty, if you would rather answer whatever you are asked.

A later `register_expansion` replaces the name list and the cache setting. Names are lowercased on
registration, so `register_expansion("Ranks", &["Prefix"])` works.

## Limits

Each of these fails loudly in the protocol rather than truncating:

| Limit | Value | Exceeding it |
| :-- | :-- | :-- |
| Message size | 1 MiB either way | `too large` |
| One line of text | 32 KiB | that request is refused |
| Namespace, name, viewer, argument | 128 characters | that request is refused |
| Lines per `set_placeholders_batch` | 8192 | the whole batch is refused |
| Expansion callbacks per line | 16 | the rest are left in the text, with a warning in the log |
| Cache TTL an expansion may ask for | 60 s | clamped to 60 s |

The callback budget is **per line** rather than per message, so a large batch is not penalised for
being large. It is counted per *occurrence*, but a placeholder repeated within one line is only
resolved once, so a line that repeats a value still costs one round trip.

## Differences from Java PlaceholderAPI

- **A callback is an IPC message, not a function call.** An expansion that is not loaded, or that
  answers with an error, leaves its placeholder in the text instead of raising.
- **Placeholder ids may not contain `-` or `.`** — only letters, digits and `_`.
- **No `%time%`, `%world_*%` or `%random_%`.** They need world and time getters the plugin WIT does
  not expose cheaply.

## Cleanup

`unregister_expansion` drops every namespace the calling plugin registered, along with anything
cached for it. It only affects the caller's own namespaces, so one plugin cannot unregister
another's.
