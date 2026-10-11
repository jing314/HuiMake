#ifndef HK_HAL_H
#define HK_HAL_H

/* 极简 HAL 抽象：示例用纯 C 实现，真实项目替换为寄存器 / 厂商库调用 */
void hal_init(void);
void hal_delay(unsigned int ms);
unsigned int hal_tick(void);

#endif
