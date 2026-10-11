#ifndef HK_HAL_H
#define HK_HAL_H

/* Minimal HAL abstraction: plain C here, register or vendor library calls in a real project */
void hal_init(void);
void hal_delay(unsigned int ms);
unsigned int hal_tick(void);

#endif
