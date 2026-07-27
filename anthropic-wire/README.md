# anthropic-wire

Serde types for the Anthropic Messages API wire format.

Unofficial, and not affiliated with Anthropic. Extracted from
[clewdr](https://github.com/Xerxes-2/clewdr), where it sits between someone
else's client and the upstream API.

## This is not a client

There are already several Anthropic client SDKs for Rust. Use one of those if
you want to *call* the API — they bring a `reqwest` client, retries and auth,
and you will be happier.

This crate is the other half: the data shapes, with nothing attached. It exists
because sitting in the middle of a conversation makes different demands than
being one end of it:

- **Unknown shapes survive.** `ContentBlock::Unknown`, `Tool::Raw` and the
  `extra` maps on `CustomTool`/`KnownTool` keep JSON this crate does not model,
  so a request can be deserialized, inspected, and forwarded without silently
  dropping fields it did not recognise. A client can ignore what it does not
  understand; a proxy has to hand it on intact.
- **Hint fields degrade instead of failing.** An unparseable `thinking` block
  yields `None` rather than rejecting the whole request, because a field the
  upstream treats as advisory should not turn into a 422 you invented.
- **Everything round-trips.** Every type is both `Serialize` and `Deserialize`,
  including the streaming events, which a client would only ever read.

Nothing here knows about transport, authentication or routing.

## Usage

```toml
[dependencies]
anthropic-wire = "0.1"
```

Dependencies are `serde`, `serde_json` and `uuid`.

The optional `token-count` feature adds `count_tokens` on `CreateMessageParams`
and `CreateMessageResponse`. Anthropic does not publish its tokenizer, so these
are estimates via `tiktoken-rs` with `o200k_base` — fine for rate accounting,
not for anything that has to agree with a bill. It is off by default because it
pulls in BPE tables.

## Stability

Version 0.1 and pre-1.0 semver: the Anthropic API gains fields regularly, and
this crate follows it. Enum variants and struct fields will be added in minor
releases. Pin accordingly.

## License

AGPL-3.0, inherited from clewdr. If that is the only thing stopping you from
using it, open an issue — relicensing is possible but needs contributor
sign-off, and it is not worth asking for in the abstract.
