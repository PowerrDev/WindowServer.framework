# ArmOS -> sevOS Migration

This document will track conceptual ports from the old ArmOS WindowServer.

| ArmOS concept | sevOS Rust destination |
|---|---|
| `surface_t` | `surface::Surface` |
| `window_t` | `window::Window` |
| `window_server_t` | `server::WindowServer` |
| rectangle helpers | `geometry::Rect` |
| compositor | `compositor::Compositor` |
| damage tracking | `damage::DamageRegion` |
| cursor | `cursor::Cursor` |
