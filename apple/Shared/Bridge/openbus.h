// The game library's C functions the Swift app calls (crates/omsi-app/src/apple_bridge.rs,
// crates/omsi-app/src/ios.rs). The functions the game calls back are Swift's own
// (`@_cdecl`, apple/iOS/GameHost.swift).

#ifndef OPENBUS_H
#define OPENBUS_H

#include <stddef.h>
#include <stdint.h>

/// A launcher command (`init`, `tick`, `lists`, `lines`, `map_overview`, `act`, `cli` ...) with
/// its JSON argument; the reply is JSON (`{"ok": true, "value": ...}` or `{"ok": false,
/// "error": "..."}`), freed with `openbus_bridge_free`.
char *openbus_bridge(const char *cmd, const char *json);
void openbus_bridge_free(char *reply);

/// The bus preview drawn by the game's renderer: 1 when a new picture (RGBA, `*out_w` x
/// `*out_h`) was written into `out`, 0 when the last one stands.
int32_t openbus_preview_frame(uint32_t w, uint32_t h, uint8_t *out, size_t cap, uint32_t *out_w, uint32_t *out_h);
void openbus_preview_orbit(float dx, float dy);
void openbus_preview_zoom(float k);
void openbus_preview_view(uint32_t view);

#if defined(OPENBUS_IOS)
/// The app's `main`: runs the game's event loop (UIKit's run loop), never returns.
void openbus_ios_main(void);
/// The scene is connected: the game may put its window on it.
void openbus_ios_scene_ready(void);
#endif

#endif
