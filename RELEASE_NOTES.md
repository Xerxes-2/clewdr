# Release Notes

## Models

- Added Claude Fable 5, Opus 5, Opus 4.8, Opus 4.7, Sonnet 5 and Haiku 4.5 to the model list.
- Removed models Anthropic has retired: Claude 3.7 Sonnet, Sonnet 4 and Opus 4. Requests to these fail upstream, so listing them only misled clients.
- Removed Opus 4.1, which Anthropic retires on 2026-08-05.
- Every model is still offered in a `-thinking` variant.

## Request handling

ClewdR now adjusts requests to match what the target model accepts, instead of forwarding parameters that are rejected with a 400.

- `temperature`, `top_p` and `top_k` are dropped on Claude 4.7 and later, which no longer accept them, and on any model where thinking is active.
- `temperature` and `top_p` are no longer sent together, which Claude 4.1 and later reject.
- Thinking is converted rather than dropped: a token budget becomes adaptive thinking on models that only support adaptive, and adaptive becomes a budget on models that only support the older form.
- `output_config.effort` is stepped down to the nearest level the model implements, and `xhigh` is only sent to Claude 4.7 and later.
- Thinking budgets are held within the range the API accepts, rather than being passed through as given.

## Fixes

- OpenAI-compatible requests using `reasoning_effort` failed with a 400. The value was being sent as a thinking budget, and `low` mapped to 256 tokens, below the API minimum of 1024. Effort levels are now mapped properly, including `xhigh` and `max`.
- The `-thinking` suffix now also requests the reasoning text on Claude 5 models, which otherwise think without returning it.

## Frontend

- Increased the interface font size. The root size was below the browser default and the smallest labels rendered at about 10px, or under 10px on mobile. Nothing is now smaller than 12px, and the interface follows the browser's own font size setting.

## Also shipped in 0.12.26

These were released in 0.12.26, but its release notes did not mention them:

- The web interface was rewritten in Leptos and WebAssembly, replacing the React frontend (#149).
- `thinking.display` is preserved when forwarding requests upstream (#151).
- Improved light theme contrast.
- Removed the 7-day Opus quota tracking.

## Development

- Added `cargo xtask` for building, running and linting: `cargo xtask dev` runs the backend and the frontend with hot reload, and `cargo xtask ci` runs everything CI runs.
- Enabled `clippy::pedantic` across the workspace, and CI now checks formatting and lints.
- Rewrote both READMEs. Several documented details, including the download command and parts of the web interface description, did not match the software.

## Compatibility

- Clients pinned to Claude 3.7 Sonnet, Sonnet 4, Opus 4 or Opus 4.1 must move to a current model. `/v1/models` lists what is accepted.
- Sampling parameters sent to Claude 4.7 and later are now dropped rather than forwarded. Previously these requests failed; they now succeed without those parameters.
- Building from source now requires the frontend to be built first. Use `cargo xtask build`.
