#include "sensor.h"

#include "core_math.h"

int sensor_read(int base, int scale)
{
    return core_add(core_mul(base, scale), 1);
}

int sensor_core_version(void)
{
#ifdef CORE_VERSION
    return CORE_VERSION;
#else
    return -1;
#endif
}
