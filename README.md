# HuiMake (hk)

用 YAML 替代 CMake / Makefile 的 C/C++ 构建工具。

一个目录就是一个**模块**，模块之间自动解析依赖、拓扑排序、分层并行编译；
**工具链、交叉编译目标、优化参数全部写在 YAML 里**，Rust 代码中不存在任何写死的编译器或编译参数。

```
hk new blinky          # 生成模块骨架
cd blinky && hk build  # 编译
hk run                 # 编译并运行
```

> **English summary** — HuiMake (hk) is a YAML-driven C/C++ build tool, an alternative to
> CMake/Makefile. A directory is a *module*; modules declare their dependencies in `config.yaml`,
> and hk resolves the order, compiles in parallel and links them. Everything about compilers —
> toolchains, cross-compilation targets, optimisation profiles — lives in `hk.yaml`, never in the
> code. It rebuilds incrementally (gcc `-MMD` dependency files + command signatures), supports
> cross compilation and MCU workflows (nano/nosys specs, `--gc-sections`, linker scripts,
> `objcopy` to bin/hex, Flash/RAM usage reports), and ships four runnable example projects plus
> unit, end-to-end and example tests driven by two helper scripts. The command line, the log output
> and all error messages are in English; this document is in Chinese with the summary above.

## 特性

| 能力 | 说明 |
| --- | --- |
| 模块化构建 | 目录即模块，自动发现、依赖成图、拓扑分层、同层并行 |
| 增量构建 | gcc `-MMD` 依赖文件 + 命令行签名，改头文件只重编受影响的目标 |
| 交叉编译 | `hk.yaml` 里声明 targets：工具链、架构参数、sysroot、`-specs=`、链接脚本 |
| MCU 专用优化 | `-Os` / `-ffunction-sections` / `-Wl,--gc-sections` / nano+nosys specs / 链接脚本 / objcopy 产出 bin、hex / size 与 **Flash-RAM 占用报告** |
| 产物类型 | `executable` / `static-lib` / `shared-lib` / `object` / `header-only` |
| 常见编译场景 | 全局宏（defines/undefines）、多 include 目录、递归 glob 多源、按目标覆盖、静态库/动态库/系统库链接、`-L`/`rpath`/`whole-archive`、C/C++ 混合 |
| 可移植配置 | 内置配置用 `OS`、`EXE_SUFFIX` 等占位符，同一份 YAML 通吃 Windows / Linux / macOS |
| 明确的错误 | 模块重名、依赖缺失、循环依赖、YAML 未知字段、找不到工具链都会直接报错并给出候选值 |

## 安装

```
cargo build --release
# 产物：target/release/hk（Windows 为 hk.exe），把它放进 PATH 即可
```

## 命令

| 命令 | 说明 |
| --- | --- |
| `hk new <NAME>` | 创建模块骨架（`--kind executable\|static-lib\|shared-lib\|object\|header-only`） |
| `hk init` | 在工程根导出 `hk.yaml`（内置默认配置，可直接编辑） |
| `hk build` | 构建当前模块（在模块目录内）或整个工程（在工程根） |
| `hk run` | 构建并运行；`-- ` 之后的参数会传给程序 |
| `hk clean` | 删除构建目录（`--dry-run` 只看不删） |
| `hk config` | 打印当前生效的配置；`--target NAME` 查看某目标解析后的工具链与参数 |

构建相关参数：`-m/--mode`（配置档）、`-t/--target`（目标）、`-j/--jobs`（并行度，0=自动）、
`--module`（只构建该模块及其依赖）、`--print-plan`（只打印命令，不执行）、`--force`（全量重建）、
`-k/--keep-going`（出错后继续构建独立模块）、`-v/-vv`（详细日志）。

输出与日志（全部为英文）：
- debug 方式构建出来的 hk 默认 INFO：打印每个模块的编译/链接进度；
- **release 方式构建出来的 hk（`cargo build --release`）默认 WARN：只打印警告、错误与最终汇总**，
  发行版不会刷屏；
- `-v` 打开逐条进度并打印真正执行的命令行，`-vv` 打印全部细节，`-q` 只留错误。

## 目录结构

```
project/                     # 工程根：放 hk.yaml（可选）
├── hk.yaml                  # 工具链 / 目标 / 配置档（不写则使用内置默认值）
├── hal/                     # 模块 = 一个含 config.yaml 的目录
│   ├── config.yaml
│   ├── include/             # 对外头文件（默认公开）
│   ├── src/                 # 库源码
│   └── build/               # 构建产物：obj/ lib/ bin/
└── app/
    ├── config.yaml
    ├── bin/                 # 可执行文件源码
    └── build/
```

默认扫描 `src/` 与 `bin/` 下的 `.c/.cc/.cpp/.cxx/.S/.s`，也可以自己写 glob。

## 最小工程

```yaml
# app/config.yaml
name: app
kind: executable
std: c11
defines:
  - 'APP_TAG="v1"'
deps:
  mods: [hal]          # 同工程的模块
  system: [pthread]    # -lpthread
```

```yaml
# hal/config.yaml
name: hal
kind: static-lib
public:                # 被依赖时传播给使用者
  defines: [HAL_ENABLED=1]
```

```
hk build                 # 自动先编译 hal，再编译并链接 app
hk run                   # 运行 app/build/bin/app
```

## 交叉编译与 MCU

在工程根 `hk.yaml` 里声明目标，然后用 `-t` 选择：

```yaml
toolchains:
  arm-none-eabi:
    cc: arm-none-eabi-gcc
    cxx: arm-none-eabi-g++
    ar: arm-none-eabi-ar
    objcopy: arm-none-eabi-objcopy
    objdump: arm-none-eabi-objdump
    size: arm-none-eabi-size

targets:
  stm32f407:
    toolchain: arm-none-eabi
    cflags: [-mcpu=cortex-m4, -mthumb, -mfloat-abi=hard, -mfpu=fpv4-sp-d16,
             -ffunction-sections, -fdata-sections]
    ldflags: ["-Wl,--gc-sections"]
    specs: [nano.specs, nosys.specs]      # 等价于 -specs=nano.specs
    linker_script: link/STM32F407.ld
    entry: Reset_Handler
    post: [elf, bin, hex, map, size]      # 链接后产物
    memory_usage: true                    # 解析 Flash/RAM 占用
    runnable: false                       # MCU 产物不能在本机运行
    exe_suffix: .elf

profiles:
  mcu-release:
    cflags: [-Os, -g0, -Wall]
    defines: [NDEBUG]
    strip: true
```

```
hk build -t stm32f407 -m mcu-release
```

内置了 `cortex-m0/m3/m4/m4f/m7`、`riscv32-unknown-elf`、`xtensa-esp32-elf` 等目标模板与
`mcu-debug / mcu-release / mcu-size` 配置档，可以直接用，也可以 `hk init` 导出一份再改。

构建后会得到：

```
app/build/bin/app.elf     # 链接产物（exe_suffix 决定后缀）
app/build/bin/app.bin     # objcopy -O binary，烧录用
app/build/bin/app.hex     # objcopy -O ihex
app/build/bin/app.map     # 链接 map
```

并且打印占用报告（来自 `-Wl,--print-memory-usage`）：

```
==> app: link -> .../app.elf
app memory usage:
     FLASH       12288 B / 512 KB  (2.34%)
       RAM        3200 B / 128 KB  (2.44%)
app section sizes (text/data/bss/dec/hex):
   text    data     bss     dec     hex filename
   1024     256     512    1792     700 app.elf
```

在没有安装交叉工具链的机器上，可以先用 `--print-plan` 核对将要执行的命令：

```
hk build -t stm32f407 -m mcu-release --print-plan
```

## 配置

- 工程级：`hk.yaml` / `project.yaml`（工具链、目标、配置档），完整字段见 **[doc/configuration.md](doc/configuration.md)**
- 模块级：`<module>/config.yaml`（产物类型、源码、宏、依赖、链接）

所有 YAML 都开启了未知字段检查：写错 key 会直接报错，不会静默失效。

## 与 0.1 的差异（迁移）

| 0.1 | 0.2 |
| --- | --- |
| `premacro` | `defines`（旧字段仍可用，会打印告警） |
| `dep.mod_deps / dep.lib / dep.include` | `deps.mods / deps.system / include_dirs` |
| `compiler` 段（cc/target/sysroot/flags） | 移到工程根 `hk.yaml` 的 `toolchains` / `targets` |
| `src/` 与 `bin/` 只扫一层、include 收文件 | 递归 glob、include 收目录及其子目录 |
| `hk build --mode release` 不生效 | `-m release`（即 profiles 名）真正生效 |
| 依赖模块写在模块目录内部 | 依赖模块是**同级目录**（或相对路径） |

## 示范工程

`examples/` 下有 4 个可直接运行的示范工程，覆盖单模块、多模块与 MCU（STM32 / GD32）：

| 示例 | 场景 | 验证方式 |
| --- | --- | --- |
| [01-single-module](examples/01-single-module/hello) | 单模块可执行文件（零配置，没有 hk.yaml） | 本机真实编译并运行 |
| [02-multi-module](examples/02-multi-module) | 静态库链 + 动态库 + 可执行，`public` 接口传播 | 本机真实编译并运行 |
| [03-mcu-stm32](examples/03-mcu-stm32) | STM32F407VGT6 裸机：链接脚本 / 启动文件 / bin+hex / 内存占用报告 | `--print-plan`，无需交叉工具链 |
| [04-mcu-gd32](examples/04-mcu-gd32) | GD32F303CCT6 裸机：GD32 固件库布局 / 逐文件 `-O2` / LTO 配置档 | `--print-plan`，无需交叉工具链 |

一键验证全部示范工程（自动复制到临时目录构建，不污染仓库）：

```bash
pwsh -File scripts/verify-examples.ps1     # Windows
bash scripts/verify-examples.sh            # Linux / macOS
```

## 测试

```bash
# 全部测试 + clippy
pwsh -File scripts/run-tests.ps1           # Windows
bash scripts/run-tests.sh                  # Linux / macOS

# 等价的手工命令
cargo test                                 # 49 个单元测试 + 9 个端到端测试 + 7 个示范工程测试
cargo clippy --all-targets -- -D warnings
```

- **单元测试**：配置解析与校验、工具链/目标解析、编译参数组装（含 STM32 / GD32 交叉参数与 MCU 后处理）、glob 匹配、依赖拓扑、路径处理、增量依赖文件解析。
- **示范工程单元测试**（`src/example_tests.rs`）：把 `examples/` 当作输入，对「每个目标 × 每个配置档 × 每个模块」生成构建计划并断言关键参数，不需要任何交叉工具链。
- **端到端测试**（`tests/e2e.rs`、`tests/examples.rs`）：真实调用 gcc 编译并运行产物；缺少 gcc 时相关用例会自动跳过。
