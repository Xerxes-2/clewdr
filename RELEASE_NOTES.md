# Release Notes

## Fixes

- Claude Code requests now identify as version 2.1.258 instead of the legacy
  2.1.76 client.
- Billing attribution now reproduces JavaScript's UTF-16 sampling exactly:
  code units 4, 7, and 20 are joined before UTF-8 hashing. This keeps the
  `cc_version` suffix correct for emoji and surrogate-pair boundaries.
