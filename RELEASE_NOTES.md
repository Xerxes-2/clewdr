# Release Notes

- fix: stop stripping `context-1m` beta token at the `200000` max_tokens threshold
- feat: add Sonnet/Opus 1M tri-state mode (`auto-probe` / `enabled` / `disabled`) with probe-fail auto fallback and auto-disable
- feat: restore OpenAI `-1M` model aliases for Sonnet 4/4.5/4.6 and Opus 4.6
- fix: limit 1M probing to Sonnet 4.x and Opus 4.6 lanes
- fix: split `429` handling for 1M flow (long-context gate 429 falls back without cooling cookie; normal rate-limit 429 still cools cookie)
