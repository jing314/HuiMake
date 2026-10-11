#include "greet.h"

#include <ctype.h>
#include <stdlib.h>
#include <string.h>

const char *greet_text(void)
{
#ifdef ENABLE_SHOUT
    static char buffer[64];
    const char *source = GREETING;
    size_t i;

    strncpy(buffer, source, sizeof(buffer) - 1);
    buffer[sizeof(buffer) - 1] = '\0';
    for (i = 0; buffer[i] != '\0'; ++i) {
        buffer[i] = (char)toupper((unsigned char)buffer[i]);
    }
    return buffer;
#else
    return GREETING;
#endif
}

int greet_answer(void)
{
    return 42;
}
