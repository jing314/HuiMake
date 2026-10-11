/* 时序敏感代码：通过 compile.per_file 指定 -O2 */

static unsigned int s_reload = 1000u;

unsigned int gd32_timer_reload(void)
{
    return s_reload;
}

void gd32_delay_loop(unsigned int loops)
{
    volatile unsigned int i;

    for (i = 0; i < loops; ++i) {
        /* 空转 */
    }
}
