# Release Notes

- chore(deps): bump `wreq` to `6.0.0-rc.28` and `wreq-util` to `3.0.0-rc.10`
- fix(code-auth): restore Claude Code OAuth authorize reliability by explicitly sending cookie + `claude-code` user-agent in code exchange
- fix(code-auth): when upstream returns `permission_error: Invalid authorization`, clear cached token and retry OAuth flow automatically
- fix(compat): adapt request/client usage for `wreq` v6 API changes
