#include "hk_bsp.h"

#include "gd32_driver.h"

static unsigned int s_polls;

void bsp_init(void)
{
    gd32_system_init();
    s_polls = 0;
}

void bsp_poll(void)
{
    s_polls++;
    if ((s_polls % 500u) == 0u) {
        gd32_led_toggle();
    }
}

int bsp_led(void)
{
    return gd32_led_state();
}
