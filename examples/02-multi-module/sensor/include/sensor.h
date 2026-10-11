#ifndef SENSOR_H
#define SENSOR_H

/* Simulates one sample: (base * scale) + offset */
int sensor_read(int base, int scale);

int sensor_core_version(void);

#endif
