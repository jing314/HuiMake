#ifndef HK_BSP_H
#define HK_BSP_H

/* 板级支持：LED 闪烁状态机 */
void bsp_init(void);
void bsp_tick(void);
unsigned int bsp_led_on_count(void);

#endif
