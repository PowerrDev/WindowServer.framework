# WindowServer Architecture

```text
Applications
    │
    ▼
UIService.framework
    │
    ▼
WindowServer.framework
    │
    ▼
NXU
```

WindowServer owns screen composition, window surfaces, z-order, focus, damage, cursor, and input routing.
