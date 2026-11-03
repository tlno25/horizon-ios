#ifndef HORIZON_IOS_BRIDGE_H
#define HORIZON_IOS_BRIDGE_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Rust-owned, immutable and valid for the process lifetime. Do not free. */
uint32_t horizon_ios_abi_version(void);
const char *horizon_ios_game_version(void);

/* 0 = uninitialized, 1 = shell ready, 2 = suspended. Not game state. */
int32_t horizon_ios_initialize(void);
uint32_t horizon_ios_state(void);
/* 0=not started, 1=initializing, 2=ready, 3=failed, 4=suspended */
uint32_t horizon_ios_graphics_state(void);
/* 0=Metal probe, 1=requested, 2=constructing, 3=real renderer presented,
   4=failed and returned to the Metal probe. Never means gameplay is active. */
uint32_t horizon_ios_renderer_preview_state(void);
/* Private, opt-in Simulator CI diagnostic; never enabled on ordinary launch. */
void horizon_ios_enable_renderer_smoke(void);
uint32_t horizon_ios_renderer_smoke_enabled(void);
/* Request a single-window Winit handoff to the real Horizon boot renderer. */
int32_t horizon_ios_preview_game_renderer(void);
/* Read-only, GPU-textured preview of up to 3x3 saved chunks. Never starts gameplay.
   0=unavailable, 1=ready, 2=requested, 3=preparing, 4=visible,
   5=failed/retry available, 6=returning to title. */
uint32_t horizon_ios_terrain_preview_state(void);
int32_t horizon_ios_preview_selected_terrain(void);
int32_t horizon_ios_leave_terrain_preview(void);
/* Reset only the transient view camera, never write the saved player pose. */
int32_t horizon_ios_reset_terrain_preview_camera(void);
/* Experimental read-only 20 TPS gravity/jump preview: no save writes/ticks. */
uint32_t horizon_ios_preview_physics_enabled(void);
int32_t horizon_ios_toggle_preview_physics(void);
int32_t horizon_ios_preview_physics_jump(void);
uint32_t horizon_ios_preview_sprinting(void);
int32_t horizon_ios_toggle_preview_sprint(void);
/* Absolute normalized analog axes: positive left and positive forward. */
int32_t horizon_ios_preview_walk(float left, float forward);
uint32_t horizon_ios_touch_count(void);
uint64_t horizon_ios_touch_events(void);
/* Diagnostic SwiftUI touch pad: 0=start, 1=move, 2=end, 3=cancel.
   Actual gameplay touch events also arrive directly from Winit. */
int32_t horizon_ios_debug_touch(uint32_t phase, float x, float y, uint32_t width, uint32_t height);
/* 0=not checked, 1=resources missing, 2=manifest found, 3=preflight failed.
   1 and 2 do not mean Minecraft gameplay has started. */
uint32_t horizon_ios_engine_state(void);
/* Rust-owned, stable player name; NULL before preflight. */
const char *horizon_ios_player_name(void);
/* Home is absolute, UTF-8, borrowed for the call only (max 4096 bytes). */
int32_t horizon_ios_prepare_engine(const uint8_t *home_utf8, size_t home_bytes);
/* Must run off the main thread. The caller retains the selected ZIP. */
int32_t horizon_ios_import_resources(const uint8_t *archive_utf8, size_t archive_bytes);
/* Block registry load: 0=idle, 1=loading, 2=ready, 3=failed.
   Calls are synchronous and must run on a worker, not the UIKit thread. */
uint32_t horizon_ios_core_load_state(void);
int32_t horizon_ios_load_core_assets(void);
int32_t horizon_ios_shutdown(void);

/* iOS-only: starts Winit/UIApplicationMain on the main thread. */
int32_t horizon_ios_run(void);

#ifdef __cplusplus
}
#endif
#endif
