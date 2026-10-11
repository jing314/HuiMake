#include "gd32_driver.h"

/* 真实工程里这里会 #include "gd32f30x.h" 并调用固件库，
   示例保持零依赖，便于在没有 SDK 的机器上核对编译参数。 */

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
