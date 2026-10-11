#ifndef SENSOR_H
#define SENSOR_H

/* 模拟一次采样：返回 (基准值 * 倍率) + 偏移 */
int sensor_read(int base, int scale);

int sensor_core_version(void);

#endif
