# Release Notes

## Changes
- Remove the `-1M` and `-1M-thinking` model aliases from the model list and request handling.
- Stop forwarding client-provided `anthropic-beta` values for long-context requests. The OAuth beta header required by Claude Code authentication remains enabled.
- Remove the special long-context 429 handling; these responses now use the standard rate-limit path.
- Remove the obsolete 1M toggle styles from the frontend.

## Compatibility
- Clients should use the standard Claude model IDs without the `-1M` suffix.
- Client-provided long-context beta headers are no longer passed to Anthropic.
