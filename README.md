# WindowServer.framework

The sevOS graphical WindowServer framework and compositor. This is written in Rust but works in C because of a compatible bridge called FFI. It just calls exported C ABI functions, which never talks to WindowServer.framework internally.

Note: This project **is** for the most part, coded alongside an AI agent. I am doing this for fun and to learn a bit about how to integrate Rust in my kernel.

## Architecture

- `crates/windowserver` - platform-independent window management and composition core.
- `crates/windowserver-nxu` — NXU display/platform backend.
- `include` — future stable C ABI headers.
- `docs` — architecture and ArmOS migration documentation.
- `examples` — standalone compositor/window demonstrations.
- `tests` — integration and behavioral tests

UIService.framework is a client-facing UI framework. WindowServer.framework owns windows, surfaces, composition, focus, z-order, and input routing.

# License
MIT