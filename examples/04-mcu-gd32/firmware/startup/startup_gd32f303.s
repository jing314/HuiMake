/* 最小启动文件示范：真实项目请使用 GD32 固件库自带的 startup_gd32f30x_hd.s */
  .syntax unified
  .cpu cortex-m4
  .fpu softvfp
  .thumb

  .global g_pfnVectors
  .global Reset_Handler
  .global Default_Handler

  .section .text.Reset_Handler
  .weak Reset_Handler
  .type Reset_Handler, %function
Reset_Handler:
  ldr   r0, =_estack
  mov   sp, r0
  bl    main
  b     .
  .size Reset_Handler, .-Reset_Handler

  .section .text.Default_Handler,"ax",%progbits
Default_Handler:
  b     .
  .size Default_Handler, .-Default_Handler

  .section .isr_vector,"a",%progbits
  .type g_pfnVectors, %object
g_pfnVectors:
  .word _estack
  .word Reset_Handler
  .word Default_Handler      /* NMI */
  .word Default_Handler      /* HardFault */
  .word Default_Handler      /* MemManage */
  .word Default_Handler      /* BusFault */
  .word Default_Handler      /* UsageFault */
  .word 0
  .word 0
  .word 0
  .word 0
  .word Default_Handler      /* SVCall */
  .word Default_Handler      /* DebugMonitor */
  .word 0
  .word Default_Handler      /* PendSV */
  .word Default_Handler      /* SysTick */
  .size g_pfnVectors, .-g_pfnVectors
