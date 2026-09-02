#ifndef WINDOWSERVER_WSDISPLAY_H
#define WINDOWSERVER_WSDISPLAY_H
#include <stdint.h>
typedef struct WSDisplay {
    uint32_t *framebuffer;
    uint32_t width;
    uint32_t height;
    uint32_t stride;
} WSDisplay;
#endif
