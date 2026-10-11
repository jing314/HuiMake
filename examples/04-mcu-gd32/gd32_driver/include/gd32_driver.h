#ifndef GD32_DRIVER_H
#define GD32_DRIVER_H

/* Minimal driver abstraction: plain C here, firmware library calls in a real project */
void gd32_system_init(void);
void gd32_led_toggle(void);
int gd32_led_state(void);

#endif
