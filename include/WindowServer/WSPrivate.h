#ifndef WINDOWSERVER_WSPRIVATE_H
#define WINDOWSERVER_WSPRIVATE_H

#include <stdbool.h>
#include <stdint.h>

#include <WindowServer/WSDIsplay.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef uint32_t WSWindowID;

bool WS_Initialize(WSDisplay display);
WSWindowID WS_Create_Window(int32_t x, int32_t y, uint32_t width, uint32_t height, bool opaque, uint32_t corner_radius);
WSWindowID WS_Create_Background_Window(int32_t x, int32_t y, uint32_t width, uint32_t height);
bool WS_Destroy_Window(WSWindowID window_id);
bool WS_Move_Window(WSWindowID window_id, int32_t x, int32_t y);
bool WS_Set_Size_Limits(WSWindowID window_id, uint32_t min_width, uint32_t min_height, uint32_t max_width, uint32_t max_height);
bool WS_Resize_Window(WSWindowID window_id, int32_t x, int32_t y, uint32_t width, uint32_t height);
bool WS_Focus_Window(WSWindowID window_id);
/* Shadow styles: 0 none, 1 popup (menus, the Dock), 2 window (the default). */
bool WS_Set_Window_Shadow(WSWindowID window_id, uint32_t style);
/* Opaque except its rounded corners: interior rows are copied whole. */
bool WS_Set_Window_Opaque_Interior(WSWindowID window_id, bool opaque_interior);
/* The key window casts the darker shadow. */
bool WS_Set_Window_Key(WSWindowID window_id, bool key);
/* Physical pixels per point, x1000: shadow sizes are points. */
void WS_Set_Shadow_Scale(uint32_t permille);
bool WS_Render_Window(WSWindowID window_id, const uint32_t *pixels, uint32_t width, uint32_t height, uint32_t stride);
/* kind: 0 = Arrow, 1 = Move, 2 = Hand, 3 = NotAllowed, 4 = ResizeHorizontal,
 * 5 = ResizeVertical, 6 = ResizeDiagonalNeSw, 7 = ResizeDiagonalNwSe,
 * 8 = Text. Installs the bitmap for that one kind; every other kind keeps
 * whatever (if anything) was installed for it. */
bool WS_Set_Cursor(uint32_t kind, const uint32_t *pixels, uint32_t width, uint32_t height, uint32_t stride, int32_t hotspot_x, int32_t hotspot_y);
/* Same kind numbering as WS_Set_Cursor. */
bool WS_Set_Cursor_Kind(uint32_t kind);
bool WS_Pointer_Move(int32_t x, int32_t y);
bool WS_Pointer_Button(int32_t x, int32_t y, uint32_t button, bool pressed);
bool WS_Present(void);

#ifdef __cplusplus
}
#endif

#endif
