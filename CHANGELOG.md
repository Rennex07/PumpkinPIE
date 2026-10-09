# Changelog

## Unreleased

Protocol version `0`. Workspace version `0.2.3`.

### Fixed

- `set_placeholders_batch` no longer sends a line over 32 KiB. It checked nothing,
  so the line was sent and then dropped by the provider's filter, which returned a
  `results` array shorter than the request. Both arrays are indexed positionally,
  so every line after the dropped one received its neighbour's answer. The batch is
  now refused whole, and the caller keeps its own text.

Verified on **Pumpkin 0.2.0+26.3-26.51, Minecraft Java 26.3 (protocol 777)**, driving the server
over its console so no client is needed. Three components load together: the provider, an expansion,
and a consumer.

The expansion path, via `crates/pumpkin-pie-testexp`:

| Check | Result |
| :-- | :-- |
| `register_expansion` on two namespaces | accepted, and the applied TTL comes back as asked |
| `on_request` round trip | the expansion's answer reaches `/pie parse` |
| `Cache::Never` | `%testexp_count%` re-asked the expansion on every resolve, incrementing each time |
| `Cache::Ttl` | asked 3 times, called back **once** |
| Declining with `None` | `%testexp_nope%` stayed literal and was reported unresolved |
| Case insensitivity | `%testexp_COUNT%` resolved, reaching the expansion as `name=count` |

The consumer path, via `crates/pumpkin-pie-consumer` and its `/piecheck`, which is the direction a
tab list or scoreboard plugin actually uses:

| Check | Result |
| :-- | :-- |
| `set_placeholders` | resolved a built in and an expansion, left `%nope_nope%` literal, reported it unresolved |
| `set_placeholders` with no viewer | `%server_online%` resolved, player placeholders stayed in the text as written |
| `set_placeholders_batch` | 3 lines in one message, all resolved, in order |
| `get_placeholder_value` | a built in and an expansion returned values; an unknown id returned `None`, not an error |
| `get_registered_placeholders` | 15 entries across `player`, `server`, `testexp`, `testexpcached` |
| `unregister_expansion` from a plugin that registered nothing | returned empty and left the other plugin's namespaces intact |

The cache survives the IPC boundary, which is the part that only a real consumer can show: three
identical consumer runs asked for `%testexpcached_value%` 3 times and got **1** callback, while
`Cache::Never` placeholders got one callback per request.

Last run was clean: no errors, no warnings from any of the three plugins, no panics.

Still unverified: no real Minecraft client this round, so the player built-ins were exercised with a
name that is not online. No other Pumpkin version has been tried.
