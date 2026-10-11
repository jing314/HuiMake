# HuiMake 配置参考（v0.2）

三层配置，全部是 YAML，代码里不写死任何编译器与编译参数：

```
hk.yaml (工程根)          config.yaml (每个模块)
├── toolchains  工具链    ├── kind      产物类型
├── targets     交叉目标  ├── sources   源码 glob
└── profiles    配置档    ├── deps      依赖
                          ├── compile   编译参数
实际命令行 = toolchain × target × profile × config.yaml
                          └── link      链接参数
```

## 1. 加载与覆盖规则

1. 工具内嵌一份默认 `hk.yaml`（`src/defaults/hk.yaml`）作为基线。
2. 工程根（含 `hk.yaml` 或 `project.yaml` 的目录，会从当前目录向上查找）的同名文件在其上覆盖：
   - 标量（`build_dir` / `default_target` / `default_mode` / `jobs`）以工程文件为准；
   - `toolchains` / `targets` / `profiles` 是**按条目名整体替换**，其余条目继续沿用内置值；
   - 想修改某个内置条目，先 `hk init` 导出一份完整配置再编辑。
3. 模块（`config.yaml`）与目标、配置档叠加，得到该次构建真正生效的参数。

所有配置文件都开启了未知字段检查：key 写错会直接报错。

### 工程与模块的发现规则

`hk` 按下面的顺序确定"工程根"和"模块"：

1. 当前目录里有 `config.yaml` → 当前目录就是一个模块（在模块目录里运行）；
2. 否则从当前目录向上查找 `hk.yaml` / `project.yaml`，找到的目录就是工程根；
   一个都没有时，把当前目录（若在模块目录里运行则是它的父目录）当作工程根；
3. 从工程根向下最多 6 层扫描含 `config.yaml` 的目录作为模块，
   跳过 `build`、隐藏目录、`node_modules` / `target` 等；
4. **遇到自带 `hk.yaml` / `project.yaml` 的子目录就停下**：它属于另一个工程，
   不会被并入当前工程（并打印"跳过子工程 xxx"）。

推论：像 `examples/` 这种"多个工程摆在一起"的目录**不是**一个工程，
直接在它下面运行 `hk build` 只会构建那些不属于任何子工程的模块；
要构建某个示例请进入它自己的目录，要只构建某个模块可以用 `--module <名字>`。

同一个工程内出现重名模块会**直接报错**（不会静默覆盖），报错里会提示上面两种可能的原因。

### 占位符

字符串里可以写以下占位符，加载时按宿主平台替换（用于写一份跨平台配置）：

| 占位符 | Windows | Linux | macOS |
| --- | --- | --- | --- |
| ${OS} | windows | linux | macos |
| ${EXE_SUFFIX} | .exe | (空) | (空) |
| ${SHARED_SUFFIX} | .dll | .so | .dylib |
| ${STATIC_SUFFIX} | .a | .a | .a |
| ${OBJ_SUFFIX} | .o | .o | .o |
| ${LIB_PREFIX} | lib | lib | lib |

## 2. hk.yaml（工程级）

### 2.1 顶层

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `version` | int | 1 | 配置版本 |
| `project` | string | null | 工程名，仅用于显示 |
| `build_dir` | string | `build` | 每个模块的构建目录名（相对模块目录） |
| `default_target` | string | `host` | `hk build` 未指定 `-t` 时使用的目标 |
| `default_mode` | string | `debug` | 未指定 `-m` 时使用的配置档 |
| `jobs` | int | 0 | 并行度，0 = CPU 核数 |
| `temp_dir` | string | null | 传给编译器的临时目录（TMPDIR/TMP/TEMP）。缺省 `<模块>/<build_dir>/tmp`；配置为相对路径时相对工程根，并按模块再分子目录 |
| `toolchains` | map | 见内置 | 工具链定义 |
| `targets` | map | 见内置 | 交叉编译目标 |
| `profiles` | map | 见内置 | 构建配置档 |

### 2.2 toolchains

| 字段 | 说明 |
| --- | --- |
| `cc` | **必需**，C 编译器 |
| `cxx` | C++ 编译器（模块含 C++ 源码时使用；缺省会报错） |
| `ar` | 静态库打包器，`kind: static-lib` 需要 |
| `objcopy` | `post` 中的 bin/hex 需要 |
| `objdump` | `post` 中的 lst 需要 |
| `size` | `post` 中的 size 需要 |
| `static_prefix` / `static_suffix` | 静态库命名，默认 `lib` / `.a` |
| `shared_prefix` / `shared_suffix` | 动态库命名，默认 `lib` / 平台后缀 |
| `exe_suffix` | 可执行文件后缀，MCU 常设为 `.elf` |
| `obj_suffix` | 目标文件后缀，默认 `.o` |

没有用到的工具（例如纯 C 工程不需要 `cxx`）可以不配，只有在真正用到时才报错。

### 2.3 targets（交叉编译的核心）

| 字段 | 说明 |
| --- | --- |
| `toolchain` | **必需**，引用 `toolchains` 中的名字 |
| `cflags` / `cxxflags` | 该目标下所有模块的架构参数，如 `-mcpu=cortex-m4 -mthumb`；其中以 `-m` 开头的参数会自动同时用于链接，保证 ld 选对架构与运行库 |
| `ldflags` | 链接参数，如 `-Wl,--gc-sections` |
| `defines` | 该目标全局宏，如 `STM32F407xx` |
| `libs` | 链接时附加的系统库（写 `m` 即 `-lm`） |
| `lib_dirs` | 附加 `-L` 目录（相对工程根） |
| `specs` | `-specs=`，如 `nano.specs`、`nosys.specs`、`rdimon.specs` |
| `sysroot` | `--sysroot=` |
| `linker_script` | `-T` 链接脚本（相对工程根） |
| `entry` | `-Wl,-e,<symbol>` |
| `post` | 链接后产物：`elf` / `bin` / `hex` / `map` / `size` / `lst` |
| `memory_usage` | 传 `-Wl,--print-memory-usage` 并解析 Flash/RAM 占用（GCC 10+ / binutils 2.35+） |
| `nostdlib` / `nostartfiles` | 裸机链接 |
| `strip` | 链接时 `-s` |
| `runnable` | 产物能否在本机运行；MCU 目标应为 `false`，`hk run` 会拒绝执行 |
| `static_suffix` / `shared_suffix` / `exe_suffix` | 覆盖工具链的命名约定 |
| `jobs` | 该目标专用并行度 |

### 2.4 profiles

| 字段 | 说明 |
| --- | --- |
| `cflags` / `cxxflags` | 优化级别与警告开关，如 `-Og -g3 -Wall` / `-Os -g0` |
| `ldflags` | 链接参数 |
| `defines` | 宏，如 `DEBUG` / `NDEBUG` |
| `post` | 与 target 的 post 取并集 |
| `strip` | 链接时 `-s` |

内置配置档：`debug`、`release`、`mcu-debug`、`mcu-release`、`mcu-size`。
内置目标：`host`、`clang-host`、`cortex-m0/m3/m4/m4f/m7`、`riscv32-unknown-elf`、`xtensa-esp32-elf`。

## 3. config.yaml（模块级）

### 3.1 顶层

| 字段 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `name` | string | 目录名 | 模块名（工程内必须唯一） |
| `kind` | enum | `executable` | `executable` / `static-lib` / `shared-lib` / `object` / `header-only` |
| `std` | string | 编译器默认 | C 标准，如 `c11` / `gnu11`；不写则不加 `-std=` |
| `cxx_std` | string | 编译器默认 | C++ 标准，如 `c++17` |
| `sources` | string[] | 见下 | 源码 glob（相对模块目录） |
| `exclude` | string[] | `[]` | 排除的 glob |
| `include` | string[] | `[include]` | 对外暴露的头文件目录（相对模块目录） |
| `include_dirs` | string[] | `[]` | 仅本模块使用的额外头文件目录 |
| `defines` | string[] | `[]` | 全局宏声明，`VERSION="1.0"` → `-DVERSION="1.0"` |
| `undefines` | string[] | `[]` | `-U` |
| `deps` | map | `{}` | 依赖 |
| `public` | map | `{}` | 对外接口 |
| `compile` | map | `{}` | 编译参数 |
| `link` | map | `{}` | 链接参数 |
| `output` | string | 模块名 | 产物名 |
| `targets` | map | `{}` | 按目标覆盖 |

默认源码 glob：

```
src/**/*.c  src/**/*.cc  src/**/*.cpp  src/**/*.cxx  src/**/*.S  src/**/*.s
bin/**/*.c  bin/**/*.cc  bin/**/*.cpp
```

`include` 里的每个目录**及其所有子目录**都会加进 `-I`，并与依赖的 public 头文件目录合并去重。

### 3.2 deps

| 字段 | 说明 |
| --- | --- |
| `mods` | 本工程内的模块：写模块名、模块目录名或相对路径（如 `../hal`）；解析失败直接报错 |
| `system` | 系统库，`pthread` → `-lpthread` |
| `static` | 静态库：写路径（`third_party/libfoo.a`）直接参与链接；写裸名则 `-l<name>`，配合 `link.static_link: true` 会用 `-Wl,-Bstatic` 包裹 |
| `shared` | 动态库：路径直接链接，裸名 `-l<name>` |
| `lib_dirs` | `-L` 目录（相对模块目录） |

链接顺序自动处理：本模块目标文件 → 依赖模块产物（按依赖拓扑逆序）→ 本模块配置的库 → 系统库；
依赖超过一个时用 `-Wl,--start-group ... --end-group` 兜住循环引用。

### 3.3 public（对外接口，被依赖时传播）

| 字段 | 说明 |
| --- | --- |
| `include` | 额外对外头文件目录（`include` 本身永远对外公开） |
| `include_dirs` | 对外公开的额外目录（本模块的 `include_dirs` 默认**不**传播） |
| `defines` | 传播给使用者的宏，如 MCU 的器件宏 `STM32F407xx` |
| `cflags` | 传播给使用者的编译参数 |

### 3.4 compile

| 字段 | 说明 |
| --- | --- |
| `flags` | 追加 C 编译参数 |
| `cxx_flags` | 追加 C++ 编译参数 |
| `per_file` | 逐文件追加参数，key 支持精确路径或 glob：`src/slow.c: [-O0]`、`vendor/**/*.c: [-w]` |

### 3.5 link

| 字段 | 说明 |
| --- | --- |
| `flags` | 追加链接参数 |
| `rpath` | `-Wl,-rpath,<dir>` |
| `whole_archive` | 以 `--whole-archive` 方式链接（写路径或库名） |
| `extra_objects` | 直接参与链接的预编译 `.o` / `.a` |
| `linker_script` | `-T`（相对模块目录，覆盖 target 中的同名配置） |
| `entry` | `-Wl,-e,<symbol>` |
| `nostdlib` / `nostartfiles` | 与 target 中的同名配置取或 |
| `static_link` | 裸名静态库是否用 `-Wl,-Bstatic` 包裹 |
| `shared_flags` | 仅 `kind: shared-lib` 时追加 |

### 3.6 targets（按目标覆盖）

键是 `hk.yaml` 中 `targets` 的名字，只覆盖列出的项：

```yaml
targets:
  cortex-m4f:
    defines: [STM32F407xx, USE_HAL_DRIVER]
    cflags: [-mfpu=fpv4-sp-d16]
    sources: [startup/**/*.S]
    exclude: [src/host_only/**]
    include: [Drivers/CMSIS/Include]
    include_dirs: [third_party/hal/include]
    compile:
      flags: [-O2]
    link:
      linker_script: link/fw.ld
      flags: ["-Wl,-u,Reset_Handler"]
    deps:
      mods: [drv]
```

可用字段：`cflags`、`cxxflags`、`ldflags`、`defines`、`undefines`、`sources`、`exclude`、
`include`、`include_dirs`、`compile`、`link`、`deps`。

## 4. 构建产物与目录

```
<module>/build/
├── obj/          # 目标文件 + .d 依赖文件 + .sig 命令行签名 + .rsp 响应文件
├── lib/          # lib<name>.a / lib<name>.so|dll
└── bin/          # 可执行文件，以及 MCU 的 .elf/.bin/.hex/.map/.lst
```

- 目标文件按源码相对路径存放（`build/obj/src/sub/a.o`），不会同名覆盖。
- 命令行超过 7000 字符时自动改用 `rsp` 响应文件（规避 Windows 32K 命令行限制）。

## 5. 增量与并行

- 每个目标文件旁保存 `.sig`（完整命令行）与 `.d`（gcc `-MMD -MP` 生成的依赖）；
  命令行变化、源文件更新、依赖的头文件更新都会触发重编，否则跳过。
- 归档与链接同样按签名 + 输入文件时间判断。
- `-j N` 控制并行度；同一拓扑层内的模块、以及同一模块内的多个源文件都会并行编译。
- `--force` 忽略增量、全量重建。

## 6. 常见场景

### 6.1 链接第三方静态库与动态库

```yaml
deps:
  static: [third_party/libfoo.a]      # 直接给路径
  shared: [z, third_party/libbar.so]  # 裸名 -> -lz，路径直接链接
  system: [pthread, m]
  lib_dirs: [third_party/lib]
link:
  rpath: [third_party/lib]            # 运行期搜索路径
  whole_archive: [libplugin.a]        # 需要强制包含全部符号时
  extra_objects: [prebuilt/entry.o]
```

### 6.2 全局宏与条件编译

```yaml
defines:
  - VERSION="1.2.3"
  - ENABLE_LOG=1
  - BOARD_STM32
undefines: [NDEBUG]
```

### 6.3 同一个工程同时支持本机与 MCU

```
hk build                      # host + debug
hk build -m release           # host + release
hk build -t cortex-m4f -m mcu-release
hk build -t cortex-m4f --print-plan   # 没有交叉工具链时先核对命令
```

### 6.4 一个模块产出动态库

```yaml
name: core
kind: shared-lib
output: core
link:
  shared_flags: ["-Wl,--out-implib,build/lib/libcore.dll.a"]   # MinGW 需要导入库时
```

依赖它的模块写 `deps.mods: [core]` 即可，链接顺序与 `-L` 由工具处理。

## 7. 排错

| 现象 | 处理 |
| --- | --- |
| `未知目标 "x"，可用目标: ...` | `hk config` 查看内置目标名，或用 `hk init` 导出后添加 |
| `找不到可执行文件 "arm-none-eabi-gcc"` | 工具链未安装或不在 PATH；也可在 `hk.yaml` 里写绝对路径 |
| `模块重名` | 两个目录的 `config.yaml` 里 `name` 相同，显式改成不同名字 |
| `无法解析依赖` | `deps.mods` 里写的是模块名 / 目录名 / 相对路径，检查拼写 |
| `检测到循环依赖` | 依赖成环，拆开模块 |
| YAML 报未知字段 | 字段名写错，对照本文档 |
| 想要看真正执行的命令 | `hk build -vv` 或 `hk build --print-plan` |
| `Cannot create temporary file ... Permission denied` | 编译器没法写系统临时目录（受限环境、沙箱或 TEMP 被改写）。hk 已把 TMPDIR/TMP/TEMP 指向 `<模块>/build/tmp`，正常不会再遇到；若仍失败，在 `hk.yaml` 里把 `temp_dir` 指到可写目录 |
| `模块重名 "x"` | 同名模块。若它们属于不同工程（例如在多个工程的上层目录运行），进入具体工程目录或用 `--module` 指定；若同属一个工程，给它们设置不同的 `name`。子目录自带 `hk.yaml` 时会被当作独立工程跳过 |
| `未知模式 "xxx"` | `-m/--mode` 是构建模式（profiles 名），不是模块目录；指定模块请用 `--module NAME` |
