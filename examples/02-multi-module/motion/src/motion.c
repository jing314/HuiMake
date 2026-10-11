#include "motion.h"

#include "core_math.h"

int motion_step(int ticks)
{
    return core_add(ticks, 100);
}
