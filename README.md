# WindowServer.framework

The sevOS graphical WindowServer and compositor.

## Status

Scaffold only. No implementation has been ported yet.

## Architecture

- `crates/windowserver` — platform-independent window management and composition core.
- `crates/windowserver-nxu` — NXU display/platform backend.
- `include` — future stable C ABI headers.
- `docs` — architecture and ArmOS migration documentation.
- `examples` — standalone compositor/window demonstrations.
- `tests` — integration and behavioral tests.

UIService.framework is a client-facing UI framework. WindowServer.framework owns windows, surfaces, composition, focus, z-order, and input routing.
