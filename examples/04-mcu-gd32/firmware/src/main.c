#include "hk_bsp.h"

int main(void)
{
    bsp_init();

    for (;;) {
        bsp_poll();
    }
}
