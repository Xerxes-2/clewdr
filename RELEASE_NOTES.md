# Release Notes

- feat: forward inbound anthropic-beta to Claude Code upstream while always appending oauth beta
- refactor: remove `-1M` model suffix handling and always use the base Anthropic beta header
- refactor: remove Claude 1M auto-probing and related config/cookie/frontend fields
