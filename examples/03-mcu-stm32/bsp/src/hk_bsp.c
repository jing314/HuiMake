#include "hk_bsp.h"

#include "hk_hal.h"

#define BLINK_INTERVAL_MS 500u

static unsigned int s_last;
static unsigned int s_count;

void bsp_init(void)
{
    hal_init();
    s_last = 0;
    s_count = 0;
}

void bsp_tick(void)
{
    if (hal_tick() - s_last >= BLINK_INTERVAL_MS) {
        s_last = hal_tick();
        s_count++;
    }
    hal_delay(1);
}

unsigned int bsp_led_on_count(void)
{
    return s_count;
}
