/**
 * Copyright (c) 2026 NXU Project. All rights reserved.
 */
/**
 * File:        include/WindowServerNXU.h
 *
 * C ABI for WindowServer.framework's NXU backend.
 */

#ifndef NXU_WINDOWSERVER_NXU_H
#define NXU_WINDOWSERVER_NXU_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Compose the initial WindowServer scene into an NXU-owned ARGB8888
 * framebuffer. The caller retains ownership of the framebuffer and must call
 * the display driver's present operation after this function returns.
 */
bool windowserver_nxu_bootstrap(
    uint32_t *framebuffer,
    uint32_t width,
    uint32_t height,
    uint32_t stride
);

#ifdef __cplusplus
}
#endif

#endif /* NXU_WINDOWSERVER_NXU_H */
