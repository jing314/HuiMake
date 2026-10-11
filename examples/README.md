# 示范工程

四个可直接运行的示范工程，覆盖单模块、多模块与 MCU（STM32 / GD32）三类场景。

| 目录 | 场景 | 覆盖到的能力 | 验证方式 |
| --- | --- | --- | --- |
| [01-single-module/hello](01-single-module/hello) | 单模块可执行文件 | 零配置（**没有** hk.yaml，全部走内置默认）、默认源码扫描（src/ + bin/）、默认 include/、全局宏 | 本机真实编译 + 运行 |
| [02-multi-module](02-multi-module) | 多模块 | 静态库 → 静态库 → 可执行 的依赖链、动态库产出、`public` 接口传播（头文件目录 + 宏）、按目标覆盖、系统库 libm、增量构建 | 本机真实编译 + 运行 |
| [03-mcu-stm32](03-mcu-stm32) | STM32F407VGT6 裸机 | 自定义工具链、器件目标（Cortex-M4F + nano/nosys specs + gc-sections）、链接脚本、启动文件、objcopy 产出 bin/hex、size 与内存占用报告 | `--print-plan`（不需要交叉工具链） |
| [04-mcu-gd32](04-mcu-gd32) | GD32F303CCT6 裸机 | 自定义工具链与配置档名、GD32 固件库目录、逐文件 `-O2`、LTO 配置档 | `--print-plan`（不需要交叉工具链） |

## 快速运行

```bash
# 本机示例：真实编译并运行
cd examples/01-single-module/hello
hk build && hk run
# 输出：HELLO FROM HK (answer=42)

cd ../../02-multi-module
hk build            # 依次编译 core -> sensor -> app，并产出动态库 libmotion
hk run --bin app    # 输出：sensor=43 core=2 sqrt=4 / target=host

# MCU 示例：先看工具会执行什么（无需安装交叉工具链）
cd ../03-mcu-stm32
hk build -t stm32f407 -m mcu-release --print-plan

cd ../04-mcu-gd32
hk build -t gd32f303 -m gd32-release --print-plan
```

装好 `arm-none-eabi-gcc` 之后，把 `--print-plan` 去掉就是真实构建：

```bash
cd examples/03-mcu-stm32
hk build -t stm32f407 -m mcu-release
# 产物：firmware/build/bin/firmware.elf / .bin / .hex / .map
# 同时打印 Flash / RAM 占用与 size 报告
```

输出形如：

```
==> hal: compiling 1 file(s)
==> bsp: compiling 1 file(s)
==> firmware: compiling 2 file(s)
==> firmware: link -> .../firmware.elf
firmware memory usage:
     FLASH       12288 B / 1024 KB  (1.17%)
       RAM        3200 B / 128 KB  (2.44%)
firmware section sizes (text/data/bss/dec/hex):
   text    data     bss     dec     hex filename
   9216     256     512    9984    2700 firmware.elf
```

## 一键验证所有示范工程

```bash
# Windows
pwsh -File scripts/verify-examples.ps1
# Linux / macOS
bash scripts/verify-examples.sh
```

脚本会逐个复制到临时目录构建（不污染本目录），最后打印 PASS/FAIL 汇总表。

## 注意：examples/ 本身不是一个工程

每个示例都是独立工程（`02` / `03` / `04` 各自带 `hk.yaml`，`01` 是零配置单模块）。
直接在 `examples/` 目录下运行 `hk build` 时，`hk` 会把带 `hk.yaml` 的子目录当作
**独立工程的边界**跳过，只构建不隶属于任何子工程的模块（这里是 `01-single-module/hello`）。

所以正确姿势是进入具体示例目录，或者用 `--module` 指定模块：

```bash
cd examples
..\target\debug\hk.exe build --module hello
# 或者
cd 01-single-module/hello && hk build
```

## 说明

- 03 / 04 的 `vendor/` 目录是**占位目录**：把厂商 CMSIS / GD32 固件库头文件放进去，或修改对应模块
  `config.yaml` 里的 `include_dirs`。示例代码用纯 C 实现，因此没有厂商 SDK 也能通过 `--print-plan` 验证配置。
- 03 / 04 的启动文件是最小示范版（只保留内核向量表前 16 项），真实项目请换成 SDK 自带的完整启动文件与链接脚本。
- MCU 目标的 `runnable: false` 会让 `hk run` 直接拒绝执行，裸机 `.elf` 不能在本机跑。
