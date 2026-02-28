# Release Notes

- Requests now keep the original `system` content from clients unchanged by default (except existing cache-control scope cleanup).
- Kept `custom_system` support: when configured, it is prepended to the request `system` content.
- Dropped the `system` field when it is empty (`null`, blank string, or empty system block list) before forwarding requests.
