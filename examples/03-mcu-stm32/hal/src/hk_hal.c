#include "hk_hal.h"

/* A real project would #include "stm32f4xx.h" and poke registers here.
      This example stays dependency-free so --print-plan can check the flags anywhere. */

static unsigned int s_tick;

void hal_init(void)
{
    s_tick = 0;
}

void hal_delay(unsigned int ms)
{
    s_tick += ms;
}

unsigned int hal_tick(void)
{
    return s_tick;
}
