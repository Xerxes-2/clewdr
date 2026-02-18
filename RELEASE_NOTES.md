# Release Notes

- feat(1m): switch back to credential-level 1M state (`supports_claude_1m_sonnet/opus`) and remove global config-level 1M toggles
- feat(ui): add per-cookie 1M override controls in Cookie Status (Sonnet/Opus: auto/enable/disable)
- fix(1m): keep auto-probe fallback behavior per credential (disable lane on long-context 400/403/429 and retry without 1M header)