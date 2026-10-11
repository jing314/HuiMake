#include <math.h>
#include <stdio.h>

#include "sensor.h"

int main(void)
{
    int value = sensor_read(6, 7);

    printf("sensor=%d core=%d sqrt=%.0f\n", value, sensor_core_version(), sqrt(16.0));
#ifdef HOST_BUILD
    printf("target=host\n");
#endif
    return 0;
}
