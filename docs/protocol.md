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

## Operations

### `ping`

```json
-> {"op":"ping"}
<- {"ok":true,"protocol":0,"name":"PumpkinPIE","version":"0.1.0",
    "placeholders":11,"expansions":2}
```

`placeholders` here counts the **built ins only**. `get_registered_placeholders` returns built ins
*plus* every name an expansion declared, so the two numbers will not match.

### `get_registered_placeholders`

```json
-> {"op":"get_registered_placeholders"}
<- {"ok":true,"placeholders":[
      {"id":"player_name","description":"The viewer's name",
       "namespace":"player","source":"PumpkinPIE"}
    ]}
```

`description` is empty for contributed names, which have no room for a description on the wire.

### `set_placeholders`

`viewer` is a player name or uuid, or omitted.

```json
-> {"op":"set_placeholders","text":"%player_name%","viewer":"Steve"}
<- {"ok":true,"text":"Steve","unresolved":[]}
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
<- {"ok":true,"results":[
      {"text":"42ms","unresolved":[]},
      {"text":"5 online","unresolved":[]}
    ]}
```

Results come back in request order.

### `get_placeholder_value`

```json
-> {"op":"get_placeholder_value","id":"server_online"}
<- {"ok":true,"known":true,"value":"5"}
```

An unknown id returns `ok:true` with `known:false` and no `value`. It is not an error.

This takes an **id, not text**, so it cannot resolve a placeholder that needs an argument.

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
<- {"ok":true,"namespace":"ranks","placeholders":["prefix"],
    "cache":{"kind":"ttl","ms":5000}}
```

`cache` is `{"kind":"never"}` or `{"kind":"ttl","ms":<number>}`, and defaults to `never`.

### `unregister_expansion`

Drops every namespace the calling plugin registered, and anything cached for it. It only affects
the caller's own namespaces.

```json
-> {"op":"unregister_expansion"}
<- {"ok":true,"namespaces":["ranks"]}
```

### `on_request`

Sent by the provider to the plugin that owns the namespace, once per placeholder it cannot answer
itself. This is a budgeted call: one resolved line gets at most sixteen, and anything past that is
left in the text, so a mistake cannot loop forever.

```json
-> {"op":"on_request","namespace":"ranks","id":"ranks_prefix","name":"prefix",
    "viewer":"Steve","argument":null}
<- {"ok":true,"value":"Admin"}
```

`id` is the full placeholder as written, `name` is the part after the namespace, **lowercased**.

Reply with no `value` — or `{"ok":true}` — to decline. The placeholder is then left in the text.

## Errors

```json
<- {"ok":false,"error":"'ranks' is reserved by the provider"}
```

`ok:false` means the provider answered and refused. Anything else, including an IPC-level failure
where the provider did not answer at all, is not part of this protocol.

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
