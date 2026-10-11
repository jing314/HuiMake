/* Timing-sensitive code: -O2 comes from compile.per_file */

static unsigned int s_reload = 1000u;

unsigned int gd32_timer_reload(void)
{
    return s_reload;
}

void gd32_delay_loop(unsigned int loops)
{
    volatile unsigned int i;

    for (i = 0; i < loops; ++i) {
        /* busy wait */
    }
}
