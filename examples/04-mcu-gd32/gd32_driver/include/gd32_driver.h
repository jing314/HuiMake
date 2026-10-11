#ifndef GD32_DRIVER_H
#define GD32_DRIVER_H

/* 极简驱动抽象：示例用纯 C 实现，真实项目替换为固件库调用 */
void gd32_system_init(void);
void gd32_led_toggle(void);
int gd32_led_state(void);

#endif
