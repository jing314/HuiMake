#include "gd32_driver.h"

/* A real project would #include "gd32f30x.h" and call the firmware library here.
      This example stays dependency-free so --print-plan can check the flags anywhere. */

static int s_led;

void gd32_system_init(void)
{
    s_led = 0;
}

void gd32_led_toggle(void)
{
    s_led = !s_led;
}

int gd32_led_state(void)
{
    return s_led;
}
