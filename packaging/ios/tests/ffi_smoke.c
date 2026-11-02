#include "horizon_ios_bridge.h"
#include <assert.h>
#include <string.h>

int main(void) {
    assert(horizon_ios_abi_version() == 1);
    assert(horizon_ios_graphics_state() == 0);
    assert(horizon_ios_renderer_preview_state() == 0);
    assert(horizon_ios_renderer_smoke_enabled() == 0);
    assert(horizon_ios_touch_count() == 0);
    assert(horizon_ios_touch_events() == 0);
    assert(horizon_ios_engine_state() == 0);
    assert(horizon_ios_core_load_state() == 0);
    assert(horizon_ios_player_name() == NULL);
    const char *version = horizon_ios_game_version();
    assert(version != NULL && strcmp(version, "26.3") == 0);

    assert(horizon_ios_shutdown() == 0);
    assert(horizon_ios_state() == 0);
    assert(horizon_ios_initialize() == 0);
    assert(horizon_ios_initialize() == 0);
    assert(horizon_ios_state() == 1);
    assert(horizon_ios_shutdown() == 0);
    assert(horizon_ios_state() == 0);
    assert(horizon_ios_engine_state() == 0);
    return 0;
}
