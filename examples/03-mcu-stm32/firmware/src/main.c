#include "hk_bsp.h"

/* 裸机主循环：不会返回 */
int main(void)
{
    bsp_init();

    for (;;) {
        bsp_tick();
    }
}
