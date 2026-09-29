# Protocol

The wire format, for anyone implementing a client or a provider outside Rust. If you are writing
Rust, use [`pumpkin-pie`](consuming.md#the-api) instead.

This page is deliberately complete enough to implement against without reading the crate. The
protocol is the contract, not the plugin.

## Framing

Every message is a UTF-8 JSON object, at most 1 MiB, sent over Pumpkin's
[inter-plugin IPC][ipc] to a plugin named `PumpkinPIE`.

A request carries an `op`; a response carries `ok`, and `error` when `ok` is false. **Unknown fields
are ignored**, so a client built against an older revision keeps working against a newer provider.

The current protocol version is `0`. Call `ping` and check it before relying on anything else.

### Limits

| Limit | Value | Exceeding it |
| :-- | :-- | :-- |
| Message size, either direction | 1 MiB | the message is refused |
| One line of text | 32 KiB | in a single request, refused; in a batch, **silently dropped** |
| Lines per `set_placeholders_batch` | 8192 | the whole batch is refused |
| An id passed to `get_placeholder_value` | 128 characters | that request is refused |
| Expansion callbacks per line | 16 | the rest are left in the text, with a warning in the log |
| Cache TTL an expansion may ask for | 60 s | clamped to 60 s |

The callback budget is per **line**, not per message, so a large batch is not penalised for being
large. It counts each *distinct* placeholder once, so a line that repeats a value still costs one
round trip.

Note what is **not** enforced server-side: the provider checks the length of an id and of a line's
text, and nothing else. Namespace, name, viewer and argument lengths are not validated. The Rust
client refuses all four above 128 characters before sending, so the request never goes out — but a
client in another language has no such guard and will be believed.

### Responses carry a variant name

This trips up every first implementation, so it is worth stating plainly. A response body is
wrapped in a **one-key object named after the operation's result**, sitting alongside `ok`:

```json
{"ok":true,"resolved":{"text":"Steve","unresolved":[]}}
             ^^^^^^^^ the variant
```

Not `{"ok":true,"text":"Steve",...}`. A flat body parses as valid JSON and still fails: the provider
sees no recognised body and refuses. Every example below is written with the wrapper, because a
client that omits it will be rejected.

A failed response is the exception — `error` is a real field of the response itself, so it is not
wrapped:

```json
{"ok":false,"error":"'ranks' is reserved by the provider"}
```

## Operations

### `ping`

```json
-> {"op":"ping"}
<- {"ok":true,"ping":{"protocol":0,"name":"PumpkinPIE","version":"0.2.2",
    "placeholders":11,"expansions":2}}
```

`placeholders` here counts the **built ins only**. `get_registered_placeholders` returns built ins
*plus* every name an expansion declared, so the two numbers will not match.

### `get_registered_placeholders`

```json
-> {"op":"get_registered_placeholders"}
<- {"ok":true,"registered":{"placeholders":[
      {"id":"player_name","description":"The viewer's name",
       "namespace":"player","source":"PumpkinPIE"}
    ]}}
```

`description` is empty for contributed names, which have no room for a description on the wire.

### `set_placeholders`

`viewer` is a player **name**, or omitted. A uuid does not resolve — the provider looks players up by
name only, so sending one silently resolves nothing.

```json
-> {"op":"set_placeholders","text":"%player_name%","viewer":"Steve"}
<- {"ok":true,"resolved":{"text":"Steve","unresolved":[]}}
```

`unresolved` lists the ids that could not be resolved, in the order they first appear. A placeholder
that cannot be resolved is **left in the text as written** — it is not an error, and not an empty
string. That includes a player placeholder resolved without a viewer.

### `set_placeholders_batch`

The same, for many lines at once. This is what a tab list or scoreboard refreshing every player
wants, because it is one message instead of one per player.

```json
-> {"op":"set_placeholders_batch","requests":[
      {"viewer":"Steve","text":"%player_ping%ms"},
      {"text":"%server_online% online"}]}
<- {"ok":true,"resolved_batch":{"results":[
      {"text":"42ms","unresolved":[]},
      {"text":"5 online","unresolved":[]}
    ]}}
```

Results come back in request order — **but the array can be shorter than the request array.** A line
over the 32 KiB text limit is dropped silently, with no error and no warning, so ten requests can
come back as nine results. Do not index `results[i]` against `requests[i]`; check the lengths match
first, or send lines you know are short.

### `get_placeholder_value`

```json
-> {"op":"get_placeholder_value","id":"server_online"}
<- {"ok":true,"value":{"known":true,"value":"5"}}
```

An unknown id returns `ok:true` with `known:false` and no `value`. It is not an error.

This takes an **id, not text**, so it cannot scan a line for placeholders. It does accept an
`argument` field, which is how a placeholder that needs one is resolved over the wire:

```json
-> {"op":"get_placeholder_value","id":"player_has_permission","argument":"PumpkinPIE:use"}
<- {"ok":true,"value":{"known":true,"value":"true"}}
```

(The Rust `PieClient::get_placeholder_value` has no argument parameter and always sends `None`, so
it reports `known:false` for `player_has_permission`. That is a limitation of the client, not the
protocol.)

### `register_expansion`

`placeholders` is what the provider advertises for your namespace in `get_registered_placeholders`
and `/pie`. It is **not** a filter: the provider forwards any `%ranks_*%` to you whether you listed
it or not. Omit the list, or leave it empty, if you would rather answer whatever you are asked.

A later call replaces the name list and the cache setting. Names are lowercased on registration, so
`"namespace":"Ranks","placeholders":["Prefix"]` works. The response echoes back the TTL the provider
actually applied, after clamping.

```json
-> {"op":"register_expansion","namespace":"ranks","placeholders":["prefix"],
    "cache":{"kind":"ttl","ms":5000}}
<- {"ok":true,"registered_expansion":{"namespace":"ranks","placeholders":["prefix"],
    "cache":{"kind":"ttl","ms":5000}}}
```

`cache` is `{"kind":"never"}` or `{"kind":"ttl","ms":<number>}`, and defaults to `never`.

### `unregister_expansion`

Drops every namespace the calling plugin registered, and anything cached for it. It only affects
the caller's own namespaces.

```json
-> {"op":"unregister_expansion"}
<- {"ok":true,"unregistered":{"namespaces":["ranks"]}}
```

### `on_request`

Sent by the provider to the plugin that owns the namespace, once per placeholder it cannot answer
itself. This is a budgeted call: one resolved line gets at most sixteen, and anything past that is
left in the text, so a mistake cannot loop forever.

```json
-> {"op":"on_request","namespace":"ranks","id":"ranks_prefix","name":"prefix",
    "viewer":"Steve","argument":null}
<- {"ok":true,"on_request":{"value":"Admin"}}
```

`id` is the full placeholder as written, `name` is the part after the namespace, **lowercased**.

To decline, reply with an empty body — `{"ok":true,"on_request":{}}`. A bare `{"ok":true}` is *not*
a usable reply; it carries no body and the provider treats it as a protocol error. The placeholder
is then left in the text.

## Parsing an id

A consumer that wants to interpret a placeholder itself has to split it the same way the provider
does, and two rules govern that:

- **The namespace is everything before the *first* underscore.** `%my_plugin_rank%` has namespace
  `my` and name `plugin_rank`, not namespace `my_plugin`. This is why a namespace may not contain an
  underscore — `%my_plugin_rank%` would be forwarded to whoever owns `my`, and would not look broken.
- **`player`, `server` and `pie` are reserved.** The provider answers those itself and refuses to
  hand them to an expansion.

Ids are case insensitive, so `%Player_Ping%` and `%player_ping%` are the same placeholder.

## Errors

```json
<- {"ok":false,"error":"'ranks' is reserved by the provider"}
```

`ok:false` means the provider answered and refused. Anything else, including an IPC-level failure
where the provider did not answer at all, is not part of this protocol.

A malformed namespace is refused, and the message says which rule it broke. A namespace may contain
only lowercase letters and digits, may not begin with a digit, and may not be one of the reserved
three.

## Implementing a client in another language

The provider does not care what language you write in. The protocol is a set of JSON objects over
Pumpkin's [inter-plugin IPC][ipc], which is part of the public WIT, so every SDK Pumpkin generates
*can* reach it. Whether a given SDK exposes it conveniently is another matter, and the
`pumpkin-plugin-api` crate keeps its generated `wit` module private, so even in Rust the message
types are re-declared as `PluginId = String` and `IpcMessage = Vec<u8>`. Expect to hand-roll the
export in another language.

What that costs you today:

- **There is no client crate but the Rust one.** In another language, a call is a few lines: build
  the JSON, send it, read the reply. A TypeScript or Kotlin client module is the most useful
  contribution here.
- **A reply is a nested result.** The WIT declares `result<result<ipc-message, string>>`, so the
  outer failure means the provider was not reachable and the inner one means it refused.
- **Nothing has a timeout.** A call is synchronous, so an expansion that hangs holds up whoever
  called it. Keep expansions cheap.
- **Do not call back into the provider from inside an `on_request` handler.** It is called
  synchronously from inside the provider's own IPC handler, so re-entering it deadlocks the server.

[ipc]: https://github.com/Pumpkin-MC/Pumpkin/blob/master/crates/pumpkin-plugin-wit/v0.1/ipc.wit
