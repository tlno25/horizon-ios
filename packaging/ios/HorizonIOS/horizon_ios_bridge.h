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
/* 0=not checked, 1=resources missing, 2=manifest found, 3=preflight failed.
   1 and 2 do not mean Minecraft gameplay has started. */
uint32_t horizon_ios_engine_state(void);
/* Rust-owned, stable player name; NULL before preflight. */
const char *horizon_ios_player_name(void);
/* Home is absolute, UTF-8, borrowed for the call only (max 4096 bytes). */
int32_t horizon_ios_prepare_engine(const uint8_t *home_utf8, size_t home_bytes);
/* Must run off the main thread. The caller retains the selected ZIP. */
int32_t horizon_ios_import_resources(const uint8_t *archive_utf8, size_t archive_bytes);
int32_t horizon_ios_shutdown(void);

/* iOS-only: starts Winit/UIApplicationMain on the main thread. */
int32_t horizon_ios_run(void);

#ifdef __cplusplus
}
#endif
#endif
