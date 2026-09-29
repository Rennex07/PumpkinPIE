# Contributing

## Building and testing

```bash
cargo test
cargo clippy --target wasm32-wasip2 --all-targets
cargo build --release --target wasm32-wasip2
```

The tests cover the parts that are easy to break quietly: the token scanner, the JSON boundary, and
the expansion table. They need no Minecraft client and no server, so they are the fast feedback
loop. What they cannot check is what a real client sees, which is the one command in
[Install](README.md#from-source).

Two things worth knowing before you build:

- **`pumpkin-plugin-api` is patched to a local path.** The root `Cargo.toml` points
  `[patch.crates-io]` at `../Pumpkin`, so building the workspace needs a Pumpkin checkout as a
  sibling directory, or you need to change that line. The published crate's WIT is currently
  identical to the server's, so the patch is a build convenience rather than a correctness
  requirement — which is the reason a release build is worth having.
- **Building the whole workspace builds both plugins**, and they cannot be linked together, which is
  the point of the crate split rather than a problem with it.

## Layout

| Path | What lives there |
| :-- | :-- |
| `crates/pumpkin-pie/src/lib.rs` | Crate docs and re-exports |
| `crates/pumpkin-pie/src/protocol.rs` | The wire format, limits, validation |
| `crates/pumpkin-pie/src/tokens.rs` | Scanning and substituting `%placeholder%` |
| `crates/pumpkin-pie/src/expansion.rs` | Expansions and namespaces |
| `crates/pumpkin-pie/src/builtins.rs` | The built in placeholder table |
| `crates/pumpkin-pie/src/client.rs` | The consumer side, and `answer` for expansion authors |
| `crates/pumpkin-pie/tests/protocol.rs` | Token, protocol and registry tests |
| `crates/pumpkin-pie-plugin/src/lib.rs` | The provider crate's entry point |
| `crates/pumpkin-pie-plugin/src/plugin.rs` | The provider's IPC surface and cache |
| `crates/pumpkin-pie-plugin/src/command.rs` | `/pie`, and the test pinning its permission node |
| `crates/pumpkin-pie-testexp/src/lib.rs` | An expansion: registers namespaces and answers |
| `crates/pumpkin-pie-consumer/src/lib.rs` | A consumer: calls the client API and logs what came back |

The two extra plugins are not decoration. They are the proof that a *different* plugin can link
`pumpkin-pie` and be answered by the provider, which is the whole point of the crate split. They
build in the same workspace with no linker flags, and their `TESTEXP-ON-REQUEST` and
`CONSUMER-CHECK` log lines are how the provider and consumer paths were each verified on a live
server. `testexp` pushes values towards the provider; `consumer` pulls them back out, which is the
direction a real scoreboard or tab list plugin uses.

### Documentation

The prose lives in `README.md` and `docs/`. The code examples there are **verified by hand** — copied
into a scratch crate and built against `wasm32-wasip2` — not by a test.

There was a `tests/docs_example.rs` that claimed to do this automatically. It could not: a test is
an `rlib`, the example is a `cdylib` exporting `init-plugin`, and a test cannot export that symbol
without colliding. So the test held a hand-rewritten variant with no `Plugin` impl and no
`register_plugin!`, which proved the *signatures* still existed and nothing about the code a plugin
author copies. That was worse than no test, because it was cited as a guarantee it did not provide.

**If you change a signature in `client.rs`, rebuild the examples by hand.** `cargo test` will not
catch a stale one.

## What would help

**Pull requests are welcome.** So are issues, and so is telling me that a placeholder you need is
missing.

- **A placeholder you actually want.** `%player_world%` and `%player_display_name%` are the most
  requested. Adding them properly means a change to
  [the WIT](https://github.com/Pumpkin-MC/Pumpkin/tree/master/crates/pumpkin-plugin-wit), which
  would be a contribution to Pumpkin rather than to this repository.
- **Client libraries.** The crate is Rust only. A TypeScript, Kotlin, Go or C# client module would
  make the protocol easier to adopt, and the protocol is small enough to fit in a language binding.
  See [docs/protocol.md](docs/protocol.md#implementing-a-client-in-another-language).
- **Tests.** Anything that turns bytes into values or back, and anything that is easy to break
  without noticing.
- **A second implementation.** The protocol is the contract, not this plugin. If you would rather
  write the provider in another language, the protocol version and the operation names are the only
  things consumers depend on. That is what [docs/protocol.md](docs/protocol.md) is for, and it is
  deliberately complete enough to implement against without reading this crate.

If you send a pull request, please say which Pumpkin version you built against and whether you
tested it in game. Bug reports are most useful with the log line the provider printed.
