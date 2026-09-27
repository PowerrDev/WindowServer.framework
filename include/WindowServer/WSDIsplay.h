#ifndef WINDOWSERVER_WSDISPLAY_H
#define WINDOWSERVER_WSDISPLAY_H
#include <stdint.h>
#include <stdbool.h>

typedef bool (*WSDisplayPresentFn)(
    void *context,
    uint32_t x,
    uint32_t y,
    uint32_t width,
    uint32_t height
);
typedef struct WSDisplay {
    uint32_t *framebuffer;
    uint32_t width;
    uint32_t height;
    uint32_t stride;
    void *present_context;
    WSDisplayPresentFn present;
} WSDisplay;
#endif
