#include <stdio.h>

#include "greet.h"

int main(void)
{
    printf("%s (answer=%d)\n", greet_text(), greet_answer());
    return 0;
}
