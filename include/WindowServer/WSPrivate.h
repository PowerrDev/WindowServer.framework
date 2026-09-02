#ifndef WINDOWSERVER_WSPRIVATE_H
#define WINDOWSERVER_WSPRIVATE_H
#include <stdbool.h>
#include <stdint.h>
#include <WindowServer/WSDIsplay.h>
#ifdef __cplusplus
extern "C" {
#endif
bool windowserver_nxu_init(WSDisplay display);
bool windowserver_nxu_pointer_move(int32_t x, int32_t y);
bool windowserver_nxu_pointer_button(int32_t x, int32_t y, uint32_t button, bool pressed);
bool windowserver_nxu_present(void);
int32_t windowserver_nxu_cursor_x(void);
int32_t windowserver_nxu_cursor_y(void);
#ifdef __cplusplus
}
#endif
#endif
