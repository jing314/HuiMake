# 厂商头文件占位目录

把 ST 的 CMSIS / HAL 头文件放到这里（或把 `hal/config.yaml` 里的
`include_dirs` 改成你的 SDK 路径）。

示例的 hal/bsp 只用纯 C 实现，因此**不依赖**这个目录也能正常构建。
