# Release Notes

## Fixes

- Quoting a value in the environment locked you out. `CLEWDR_ADMIN_PASSWORD="12345"` set the password to `"12345"` with the quotes attached, so the password printed at startup was not the one the login page would take. Compose files and `.env` files pass quotes through as data rather than stripping them the way a shell does, so writing them is a common habit, and every earlier version quietly absorbed it. A value written as one quoted string is unwrapped again, and surrounding whitespace is dropped again. `CLEWDR_PASSWORD` was affected the same way. ([#157](https://github.com/Xerxes-2/clewdr/issues/157))

  A value that only contains quotes — `say "hi"`, or two of them side by side — still keeps every character, and single quotes are still part of the value. If a password genuinely needs to begin or end with a space, wrap it in double quotes.

  Setting a password that reads as a number, `CLEWDR_PASSWORD=12345`, still works. That was fixed in 0.13.3 and is unaffected; the quoting behaviour was removed alongside it by mistake, and only the quoting is being restored.
