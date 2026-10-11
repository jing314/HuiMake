#include "hk_hal.h"

/* 真实工程里这里会 #include "stm32f4xx.h" 并操作寄存器。
   示例保持零依赖，便于在没有 SDK 的机器上用 --print-plan 检查编译参数。 */

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
