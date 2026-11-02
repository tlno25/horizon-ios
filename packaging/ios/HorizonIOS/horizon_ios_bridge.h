#ifndef HORIZON_IOS_BRIDGE_H
#define HORIZON_IOS_BRIDGE_H

#include <stdint.h>

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
int32_t horizon_ios_shutdown(void);

/* iOS-only: starts Winit/UIApplicationMain on the main thread. */
int32_t horizon_ios_run(void);

#ifdef __cplusplus
}
#endif
#endif
