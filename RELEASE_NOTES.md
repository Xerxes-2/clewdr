# Release Notes

## Fixes

- Beta features a client asked for were dropped on the way through. ClewdR sent its own `anthropic-beta` header and discarded whatever the client had set, so anything gated behind a beta flag the client requested — context management among them — never reached Anthropic. The client's flags are now merged with ClewdR's own and passed on, without duplicates.

- Streamed responses failed to parse when citations were enabled. The list of delta types a response could contain was missing `citations_delta` and `compaction_delta`, both of which the API sends today, and had no fallback for anything unrecognised. The failure landed mid-response, after the reply had started arriving, where nothing already sent could be taken back. Both are now understood, and a delta type ClewdR does not know is passed through untouched rather than ending the response.

- Six request fields were accepted and then silently dropped: `cache_control`, `inference_geo`, `speed`, `diagnostics`, `fallbacks` and `fallback_credit_token`. A client setting any of them got a reply that ignored it, with nothing to indicate why. They are forwarded now, checked field by field against Anthropic's own SDK.

- The cookie listing sent OAuth access and refresh tokens to the browser. The tokens were serialized along with the rest of each cookie's status, and the page discarded them on arrival — so they crossed the wire, sat in browser buffers and any proxy log on the path, and were never read. The endpoint requires the admin password, so this was not a way in for anyone unauthenticated; what it changed is how much an intercepted admin session gives up. They are removed before the response is built.

## Performance

- Every request spent about 60ms building a tokenizer it then used for 0.04ms of work. Token counting rebuilt the 200,000-entry encoding table from scratch on each call rather than keeping it, and both request and response counting did this, so a request that counted input and output tokens paid it twice. The table is now built once per process. Counts are unchanged.

## Development

- The Anthropic request and response types live in their own crate, `anthropic-wire`, instead of sitting inside the server. Nothing about the protocol depends on ClewdR, and separating them means the compiler enforces that. It also surfaced a conversion broad enough to turn any string into an assistant message, which had gone unnoticed while everything shared one crate.

- Eight direct dependencies are gone: `oauth2`, `figment`, `moka`, `serde_with`, `itertools`, `trie-rs`, `passwords` and `const_format`. Each was doing something small enough to write directly — two OAuth grants, a config merge, two caches, three serde adapters, one message fold, a stop-sequence match, a password generator, one string concatenation. The lockfile is down from 453 packages to 389.

- Release builds no longer compile the frontend for the host on top of the target build, and CI installs its tools with `--locked`. A prebuilt binary download that timed out was falling back to building from source with freshly resolved dependencies, which picked up an incompatible pair and failed the frontend job on a commit that had not touched the frontend.
