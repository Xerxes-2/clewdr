# Release Notes

## 2026-02-28

- Removed automatic prelude injection into system prompts for non-Claude Code requests.
- Requests now keep the original `system` content from clients unchanged (except existing cache-control scope cleanup).
