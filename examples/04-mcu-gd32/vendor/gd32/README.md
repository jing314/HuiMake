# GD32 固件库占位目录

把 GD32F30x 固件库（`gd32f30x.h`、`gd32f30x_gpio.c` 等）放到这里，或修改
`gd32_driver/config.yaml` 的 `include_dirs`。

示例的 gd32_driver 只用纯 C 实现，因此**不依赖**这个目录也能正常构建。
