#ifndef HK_BSP_H
#define HK_BSP_H

/* Board support: LED blink state machine */
void bsp_init(void);
void bsp_tick(void);
unsigned int bsp_led_on_count(void);

#endif
