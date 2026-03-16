# Release Notes

- Fix: added the `Adaptive` variant to the `Thinking` enum.
- Exposed `custom_system` in the web Settings UI, and clarified that `custom_prompt` is only for the Claude Web `prompt` field.
- Claude Code now emulates `claude-code/2.1.76` more closely, including `User-Agent: claude-code/2.1.76` on Claude Code requests.
- Claude Code requests now prepend a 2.1.76-compatible billing system header before `custom_system`, using sampled characters from the first user message and fixed `cch=00000`.
