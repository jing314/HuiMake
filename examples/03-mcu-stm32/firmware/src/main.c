#include "hk_bsp.h"

/* Bare-metal main loop: never returns */
int main(void)
{
    bsp_init();

    for (;;) {
        bsp_tick();
    }
}
