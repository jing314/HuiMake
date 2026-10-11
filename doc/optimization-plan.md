# HuiMake(hk) 代码审阅与优化计划

> **状态（v0.2 已实施）**：本文件是改造前对 0.1 的审阅记录，其中列出的 P0/P1 问题已在 0.2 全部修复，
> 保留作为背景参考。当前的配置与用法请看 [configuration.md](configuration.md) 与 [../README.md](../README.md)，
> 文末"v0.2 修复对照"给出了逐条落地情况。

> 审阅对象：`tool/HuiMake`（Rust，package 名 `hk`，v0.1.0，10 个源文件约 1000 行）
> 审阅方式：全量通读 + 本机实测（`cargo check` / `cargo build` 通过，随后用真实项目复现运行行为）
> 结论摘要：**设计意图清晰、代码量小；但当前版本存在若干"结构性不可用"缺陷——工具连自己 `hk new` 生成的模块都构建不过去，release 模式完全不编译，跨模块依赖的路径假设与 README 描述相反。建议先做 P0 修复让主路径真正跑通，再按阶段重构。**

---

## 一、当前功能梳理

### 1.1 定位

一个用 YAML 描述、替代 CMake/Makefile 的 C 项目构建工具（README 声称 C/C++，实际只处理 `.c`）。

### 1.2 目录与命令

| 模块 | 文件 | 职责 |
| --- | --- | --- |
| CLI 入口 | `src/main.rs` (95) | clap derive 定义 `build [-m/--mode]` / `clean` / `run` / `new [name]`，分发到 `CmdCtx` |
| 命令上下文 | `src/cli/context.rs` (139) | `detect_env` 判断"项目根 / 模块根"，持有 `ModMgr` + 当前 `ModFile`，编排 build/run/clean/gen |
| 依赖管理 | `src/module/manager.rs` (109) | petgraph `DiGraph<String,()>` 建模块依赖图；反复取"入度为 0"的节点实现拓扑分轮 |
| 模块实体 | `src/module/mod_file.rs` (513) | `ModFile`（扫描目录 + 编译 + 链接 + 运行）、`ProjectMap`（发现子模块） |
| 配置 | `src/utils/yaml.rs` (89) | `Config { name, std, premacro, dep, compiler }`、`Dep { include, mod_deps, lib }`、`Compiler { cc, target, sysroot, flags }` |
| 日志/横幅 | `src/utils/log.rs` (39)、`logo.rs` (7) | `logd!/logi!/loge!`，仅在 `debug_assertions` 下生效；ASCII LOGO |

### 1.3 构建流程（实际执行路径）

```
hk <cmd>
 ├─ new   : ModFile::gen(name)  → 建 bin/ include/ src/ + config.yaml
 └─ 其它  : CmdCtx::detect_env()
             ├─ 试 ModMgr::build_dep_graph(".")  成功 → ProjectRoot
             └─ 否则试 ModFile::load(".")       成功 → ModRoot（再对 ".." 建图）
                  └─ build(mode) / run() / clean()
                       └─ 循环 ModMgr::get_next_buildable()（反复摘除入度 0 的节点）
                            └─ 每个 ModFile::build()
                                 ├─ init_build_dirs()      build/{bin,object,lib}
                                 ├─ build_lib()            cc crate 编译 src/** → 静态库 → rename 到 build/lib
                                 ├─ build_bin()            gcc -c bin/*.c → build/object/*.o
                                 └─ link_executables()     gcc -L… obj 本模块库 依赖库 -o build/bin/<stem>
```

依赖模型：`config.yaml` 的 `dep.mod_deps` 列依赖模块名，模块是**项目根目录下的同级子目录**（README）；`dep.include` 列额外头文件路径；`dep.lib` 列系统库（渲染成 `-lxxx`）。

---

## 二、实测结论（本机复现，非静态推断）

环境：Windows + MinGW gcc + cargo（离线 registry 副本）。全部命令在临时目录执行，验证后已清理。

| # | 复现步骤 | 实测结果 | 判定 |
| --- | --- | --- | --- |
| 1 | `hk new m1`，查看 `m1/config.yaml` | 内容为 `name: app`（不是 `m1`） | 生成即错名 |
| 2 | `cd m1 && hk build` | `src/` 为空仍然去构建静态库 → `ar: ...libapp.a: No such file`，**exit 1** | 开箱即失败 |
| 3 | 两个 `hk new`（m1/m2）后 `hk build` | 两个节点都叫 `app`，只有 `m2/build` 生成，m1 被静默吞掉 | 重名静默遮蔽 |
| 4 | 同级两模块 appm → libm，`hk build` | 头文件按 `appm/libm/include`（模块**内部**）解析，日志 `Failed to canonicalize dependency include`；`dep_libs` 同样拼 `appm/libm/build/lib/lib libm.a` | 依赖路径假设与 README 相反 |
| 5 | 观察传给 gcc 的 `-I` 参数 | `-I ...\libm\include\mathx.h`（传的是**文件**）；gcc 报 `not a directory`，随后 `#include "mathx.h"` fatal error: No such file | include 路径收集错误 |
| 6 | 单独验证 `gcc -I <某个 .h 文件>` | `cc1.exe: warning: ...mathx.h: not a directory` | 同上，独立佐证 |
| 7 | `hk build --mode release` | 打印 `Release Mode`，**exit 0，什么都没编译**（build 目录未创建） | release 是空操作 |
| 8 | `hk build --mode foo` | 静默按 debug 编译，不报错 | 参数校验缺失 |
| 9 | `hk run`（项目根） | 按拓扑序对**每个**模块都执行其全部可执行文件 | run 语义不对 |
| 10 | 观察库编译 | `cc` crate 找不到 `cc`（`config.compiler.cc: gcc` 被忽略），且强制 `--target x86_64-unknown-linux-gnu`、注入 `-O2 -fPIC -ffunction-sections -m64` | 库与目标文件用不同编译器/目标 |
| 11 | 观察 stdout | 满屏 `cargo:rerun-if-env-changed=...`、`TARGET = None`（cc-rs 的 build-script 指令）泄漏到用户终端 | cc crate 用错场景 |
| 12 | 观察路径 | `\\?\C:\...` verbatim 路径被交给 gcc/ar；`ar` 直接因该路径报错 | Windows 路径未归一化 |
| 13 | `detect_env` 日志 | 对 `.` 下的**每个条目**（含 `config.yaml`、`bin`、`src`）都执行一次完整 `load`；同一模块在发现阶段被加载 **2 次** | 探测昂贵且靠试错 |
| 14 | `cargo check` | 通过，但有 5 处 unused import 警告 | 基础卫生 |

（1–3 号问题在 Linux 上同样成立：`hk new` 生成 `name: app` 与空 `src/` 依然会让 `hk build` 失败，因为 `src/` 存在时 `load_all_files` 返回空的 `Some(vec![])`，`lib_sources.is_none()` 恒为 false。）

---

## 三、问题清单

### P0 — 阻断主路径（必修）

| ID | 位置 | 问题 | 建议 |
| --- | --- | --- | --- |
| P0-1 | `mod_file.rs:123-135` + `yaml.rs:64-76` | `hk new <name>` 写死 `name: app`，`gen` 忽略 `mod_name`；且 `if let Ok(yaml)` 吞掉写盘失败 | `Config::new(name)` 接受名字；`gen` 传播错误；同时落 `main.c` 模板与 `.gitignore` |
| P0-2 | `mod_file.rs:159-163` | `lib_sources` 恒为 `Some`（空目录也返回 `Ok(vec![])`），空源码也去编译/打包静态库 → `ar` 失败 | 判空：来源为空则直接返回 `Ok(None)` |
| P0-3 | `main.rs:61-76` | `--mode release` 只打印不构建；`--mode` 同时有 `default_value` 与 `Option`，`None` 分支是死代码；非法值静默降级 | 引入 `BuildMode` 枚举（clap `ValueEnum`），把 mode 一路传到编译器 |
| P0-4 | `mod_file.rs:276,401` | 依赖模块路径用 `self.absolute_path.join(dep)`（= 模块**内部**），与 README/建图的"同级目录"模型矛盾 → 头文件与库都找不到 | 由 `项目根 + 依赖名` 统一解析，`Project::resolve_dep()` 单一入口 |
| P0-5 | `mod_file.rs:48-50` + `391-424` | `include_paths` 收集的是 `include/` 下的**文件**而非目录；`-I <file>` 无效，`#include` 直接失败 | 收集目录：`include/` 及其所有子目录（递归） |
| P0-6 | `mod_file.rs:48-50,427-464` | 源文件收集不递归、不按扩展名过滤：`src/` 下任何文件都被当 C 源；`bin/` 只扫一层 | 只收 `.c/.cc/.cpp/.S`，递归 + 稳定排序 |
| P0-7 | `mod_file.rs:200-207,278` | 库命名两端不一致：`build_lib` 在 Windows 产出 `{name}.lib`，`dep_libs` 永远找 `lib{name}.a`；`fs::rename` 在 Windows 覆盖已存在文件会失败 | 抽出平台化的库名/`.exe` 后缀函数；rename 前先删除目标 |
| P0-8 | `mod_file.rs:45,503` | `fs::canonicalize` 产出 `\\?\` 前缀路径并传给外部工具，Windows 下 `ar` 直接失败 | 统一 `path::normalize()`（Windows 剥离 verbatim 前缀，或引入 `dunce`） |
| P0-9 | `manager.rs:48-52` | 依赖名在图里被 `clean_mod_name`，在编译/链接阶段却用原文；依赖不在图里时**静默忽略** | 归一化只做一次并落库；解析不到的依赖 → 硬错误并列出模块名 |
| P0-10 | `manager.rs:35-39` + `mod_file.rs:504-510` | 模块名取自 `config.yaml` 的 `name`，重名时后写覆盖先写，一个模块被构建两次、另一个永不构建（实测 #3） | 发现阶段校验重名/重路径并报错；`name` 与目录名不一致时告警 |
| P0-11 | `yaml.rs:54-61`（及 `Dep`） | `name/std/premacro/dep` 无 `#[serde(default)]`，最简 `config.yaml` 解析失败 → `is_mod_dir` 返回 false → 模块被当作不存在 | 全部字段给默认值 |
| P0-12 | `mod_file.rs:222,297` | `build_lib` 用 cc crate，绕过 `config.compiler.cc/target/sysroot/flags`；`build_bin` 用 gcc。库与目标文件由不同编译器/目标产出；cc-rs 强制 linux target 并往 stdout 喷 `cargo:` 指令 | 弃用 cc crate，统一 `cc -c` + `ar rcs` 命令行后端（见 4.3 D1） |

### P1 — 设计缺陷（影响可维护性与正确性）

1. **三套并行数据结构**：`ModMgr.graph`、`ProjectMap.mod_names`（Vec）、`indices`（HashMap）、`index`（HashMap，`mod_file.rs:471,508`，**全项目无任何读取，死字段**）。`mod_names` 也是 `indices.keys()` 的冗余。
2. **图是破坏性遍历**：`get_next_buildable` 摘除节点，`ModMgr` 只能走一遍；`build` 与 `clean` 因此各写一份循环，`context.rs:60-132` 两大段近似复制粘贴。
3. **`detect_env` 靠试错**：先试图建图、失败再试图当模块（`context.rs:36-53`），把"配置错误/依赖成环"和"这里是模块根"混为一谈；同一模块被加载两次（`is_mod_dir` + `load`）。应以标记文件判定。
4. **错误处理**：全项目 `Box<dyn Error>` + `"..."` 字面量 `.into()`，`main` 返回 `Err` 时 Rust 打印 `Error: "msg"`（带引号的 Debug），无模块名/阶段上下文；`load` 里的 `.ok()`（`mod_file.rs:48-53`）把 IO 错误静默吞掉。
5. **日志体系**：`loge!` 走 `println!`（stdout），`logd!` 走 `eprintln!`；三者只在 `debug_assertions` 生效 → **release 版本连错误都不打印**；`format!` 与 `format_args!` 混用。
6. **无增量构建**：每次全量重编译，无 `-MMD` 依赖文件与 mtime 比较；`build/object` 同时被 cc-rs 与手写 `gcc -c` 使用（`mod_file.rs:172,213`），同名 `.o` 会互相覆盖。
7. **链接不完整**：`dep_libs` 只取**直接**依赖，且顺序按 `mod_deps` 书写顺序而非拓扑序（`mod_file.rs:270-284,299-341`）→ 传递依赖 undefined reference；GNU ld 单遍扫描静态库，顺序敏感。
8. **确定性**：`fs::read_dir` 顺序依赖文件系统，模块顺序、编译顺序、链接顺序都不可复现。
9. **`ModFile` 职责过大**（513 行）：数据模型 + 目录发现 + 编译器调用 + 链接 + 进程执行 + 脚手架生成混在一个 `impl`。
10. **输入未校验**：`hk new ../../x` 可写出工作区；`std` 未做白名单；`flags`（含 `-o` 之类）可破坏自身输出。
11. **命令构造重复两遍**：`build_bin` 与 `link_executables` 各自拼一遍 flags，集合还不一致（bin 有 `--sysroot`/`flags`，lib 没有；lib 有 `opt_level`，bin 没有）。
12. **mode 完全不落地**：debug/release 对编译参数零影响（`opt_level(2)` 写死）。

### P2 — 能力缺口（README 的 ToDo）

- C++ 支持缺失（只扫 `.c`，`.std` 默认 `c99`，无 `cpp` 开关）；README 却写"C/C++"。
- 二进制与源文件一一对应（一个 `bin/*.c` = 一个可执行文件），无法多源/多模块合成一个可执行文件，也无法指定目标名。
- 只有静态库，无共享库；无 `install`、无 `test`、无 `--bin`/多目标选择。
- `hk run` 只支持"构建并运行所有可执行文件"，不能传参、不能选目标，cwd 是调用目录而非模块目录（`mod_file.rs:99`）。
- 无并行（make -j / ninja 式）；`get_next_buildable` 天然给出可并行批次，白白浪费。
- 无缓存后端探测（ccache/sccache）、无 `clean` 粒度。
- 平台：`cfg!(windows)` 只覆盖了库文件名一处，其余（`.exe` 后缀、`ar/ranlib`、`\\?\`、分隔符）全是 Unix 假设；README 说"只支持 linux"但代码又保留半套 Windows 分支 —— 现状是**哪边都不完整**。
- 无测试、无 CI、无 `rustfmt.toml`/clippy 配置；`doc/readme.md` 与 `README.md` 重复（前者少一段）。
- 版本号两处不一致：`Cargo.toml` 的 `0.1.0` vs `main.rs:15` 硬编码的 `1.0.0`。

---

## 四、优化计划

### 4.1 阶段划分与验收

| 阶段 | 目标 | 关键验收（必须能跑） | 预估 |
| --- | --- | --- | --- |
| **S0 止血** | 主路径可用：`hk new m1 && cd m1 && hk build` 成功；release 真的构建 | P0-1/2/3/7/11/12；`hk build -m release` 产出且 `hk run` 成功 | 1 天 |
| **S1 正确性** | 跨模块依赖真的能编译、链接、运行 | P0-4/5/6/8/9/10；两模块 appm→libm 端到端跑通并打印 `3` | 1–2 天 |
| **S2 重构** | 拆模块、统一编译器后端、错误/日志体系、去 petgraph | `cargo clippy -D warnings` 干净；单测覆盖图/配置/路径；`-v` 才有详细日志，错误含模块与阶段 | 3–5 天 |
| **S3 能力** | 增量构建、并行、传递依赖链接、C++、多目标 | 二次构建 "nothing to do"；改 1 个源只重编 1 个；改头文件只重编受影响目标 | 1 周 |
| **S4 工程化** | 测试/CI/文档/跨平台策略定稿 | `cargo test` 在 linux+windows 通过；README 与实际行为一致 | 3–5 天 |

### 4.2 目标结构

```
src/
  main.rs                 # 只做 CLI 解析与分发
  cli/mod.rs  cli/args.rs # clap 定义（BuildMode: ValueEnum）
  error.rs                # thiserror: HkError { Io, Yaml, Config{file}, Build{module,stage}, Cycle{..}, DepMissing{..} }
  config/mod.rs           # Config/Dep/Compiler，全字段 serde(default) + validate()
  project/mod.rs          # 发现：ProjectRoot / ModRoot 显式判定；一次性加载，不重复 load
  project/graph.rs        # Kahn 拓扑分层 + 环检测（自实现约 60 行，去掉 petgraph）
  model/module.rs         # ModInfo（纯数据，无 IO 无 Command）
  build/plan.rs           # BuildPlan：目标(可执行) <-> 源文件 <-> 依赖库闭包
  build/flags.rs          # 统一的编译参数生成（lib 与 bin 共用）
  build/compile.rs        # 编译（含 -MMD -MP 依赖文件）
  build/link.rs           # 链接：传递闭包 + 拓扑序 + --start-group
  build/incremental.rs    # mtime + .d 依赖文件
  util/path.rs            # normalize()（去 \\?\、统一分隔）
  util/process.rs         # run(cmd, ctx) -> 失败时打印完整命令行 + 退出码
  util/log.rs             # log/env_logger，error→stderr，release 保留
```

### 4.3 关键设计决策

**D1. 统一编译器后端，弃用 `cc` crate。**
cc-rs 是为 `build.rs` 设计的：它往 stdout 吐 `cargo:` 指令、强制推断 target/host、注入 `-O2 -fPIC`、忽略 `compiler.cc`（实测 #10/#11）。自己拼命令行反而更短、更可控，更符合"轻量替代 CMake"的定位：

```rust
// build/flags.rs —— lib 与 bin 共用
pub fn base_args(cfg: &Config, mode: BuildMode, includes: &[PathBuf]) -> Vec<OsString> {
    let mut a = Vec::new();
    a.push(format!("-std={}", cfg.std).into());
    a.extend(mode.args());                        // Debug: -O0 -g ; Release: -O2 -DNDEBUG
    a.extend(cfg.premacro.iter().map(|d| format!("-D{d}").into()));
    a.extend(includes.iter().flat_map(|i| [OsString::from("-I"), i.into()]));
    if let Some(s) = &cfg.compiler.sysroot { a.push("--sysroot".into()); a.push(s.into()); }
    a.extend(cfg.compiler.flags.iter().map(Into::into));
    a
}
// 库：逐个 compile_object() 后 ar rcs <lib> <objs>；不要再用 cc::Build
```

**D2. 依赖解析只有一个入口。**
`Project { root, modules: IndexMap<String, ModInfo> }`，`resolve_dep(&self, name)` 找不到即硬错误。头文件路径 = `include/` 及其**所有子目录** + 依赖闭包的 include 目录 + `dep.include`。

**D3. 拓扑序算一次，反复使用。**
`graph::topo_levels()` 返回 `Vec<Vec<ModId>>`（同层可并行）；成环时返回具体环路成员用于报错。build/clean/run 共用同一份 plan。

**D4. 平台策略二选一，不要半吊子。**
推荐：先明确 **Unix 优先**（其他平台启动即清晰提示"暂不支持"或 `compile_error!`），把 Windows 作为 S4 的完整特性一次性做完 —— 需同时处理 `.exe` 后缀、`ar/ranlib`、`\\?\` 前缀、`lib` 前缀。当前"有分支但全错"的状态最危险。

**D5. 增量构建（性价比最高的一项能力）。**
`build/object/<mod>/<相对路径>.o`（按相对路径分目录，消除同名覆盖）；编译带 `-MMD -MP -MF <obj>.d`；构建前比较 `max(mtime(源), mtime(.d 里所有依赖)) > mtime(obj)` 才重编。顺带解决"改了 `.h` 不重编"。

**D6. 链接顺序。**
可执行目标 = `[本模块库] + 依赖闭包按拓扑逆序`；Linux 上再兜一层 `-Wl,--start-group … --end-group` 消除静态库环形引用。

**D7. 可复现。**
所有 `read_dir` 结果排序；`BTreeMap`/`IndexMap` 替代 `HashMap`（模块、宏、flags）；命令行参数顺序固定。

**D8. 配置演进。**
保持 `config.yaml` 向后兼容（全字段默认值），新增可选 `project.yaml`（项目根）声明模块清单/全局 flags/默认 mode。这样可顺手消灭 P0-10（重名）与 P1-3（试错探测）：项目根 = 存在 `project.yaml`，模块根 = 存在 `config.yaml`。

### 4.4 分任务清单（可直接开工）

**S0（止血）**
1. `yaml.rs`：所有字段加 `#[serde(default)]`，`Config::new(name)`；补 `std` 白名单校验。
2. `mod_file.rs::gen`：用目录名写 `name`；落 `bin/main.c` Hello 模板、`build/` 的 `.gitignore`；错误传播。
3. `build_lib`：源为空直接 `Ok(None)`；库名/扩展名走平台函数；rename 前删除目标。
4. `main.rs`：`BuildMode` 枚举（`ValueEnum`，默认 debug），release 走真实构建路径；版本号改 `env!("CARGO_PKG_VERSION")`；清 5 处 unused import；`loge!` 在 release 保留并走 stderr。
5. 冒烟测试：临时目录 → `hk new` → `hk build` → `hk run` 断言退出码 0。

**S1（正确性）**
6. `util/path.rs::normalize`（Windows 剥离 `\\?\`），所有进入 `Command` 的路径统一过一遍。
7. `Project::resolve_dep`：依赖模块 = 项目根下的同级目录；`merge_includes`/`dep_libs` 全部改走它；解析失败即报错（列出缺失模块名与引用方）。
8. `collect_dirs`（递归目录）与 `collect_sources`（递归 + 扩展名白名单 + 排序），替换 `load_all_files`/`load_c_files`。
9. 发现阶段：重名/同名不同路径 → 硬错误；`is_mod_dir` 改为"存在 `config.yaml`"的廉价判定，避免重复 `load`。
10. 端到端测试 `tests/e2e_project.rs`：用 `tempfile` 构造 libm+appm，断言 `hk build && hk run` 输出 `3`。

**S2（重构）**
11. `error.rs`（thiserror）+ `HkError::context(module, stage)`；`util/process.rs` 失败时输出完整命令行与退出码。
12. `log` crate + `env_logger`：默认 `warn`，`-v/-vv` 提升；`print_logo` 只在顶层打印一次（现在每个模块都打）。
13. 拆 `ModFile` → `ModInfo`（数据）+ `build/*`（行为）；`ModMgr` + `ProjectMap` 合并成 `Project`（保留一个 `IndexMap`，删死字段 `index`）。
14. 自实现 Kahn 拓扑排序替换 petgraph（少一个依赖，天然支持"环成员列表"报错 + 分层并行）。
15. 统一 `flags.rs`，删掉 `build_bin`/`link_executables` 里重复的 flag 拼装与 `cc` 依赖。
16. `context.rs` 的 build/run/clean 三份近似循环合并为一个 `for level in &plan.levels` + 目标过滤（`Target::Only(cur)` / `Target::All`）。

**S3（能力）**
17. 增量构建（`-MMD -MP` + mtime）与 "nothing to do" 提示。
18. 批次内并行编译（`rayon` 或按 `-j` 起线程），错误聚合。
19. 依赖闭包链接 + `--start-group`。
20. C++（扩展名 → 编译器与标准映射）、共享库开关、多源/多目标（`bin` 下按子目录或 targets 声明分组）、`hk run --bin x -- args`、`install`、`clean --all`。
21. 缓存后端探测（`CCACHE`/`SCCACHE`）+ `hk build --print-plan`（输出将执行的命令，便于排查）。

**S4（工程化）**
22. 单测（图/配置/路径/闭包/过滤）+ `tests/` 端到端 + CI（`fmt --check`、`clippy -D warnings`、`test` on linux/windows）。
23. 文档：合并 `README.md` 与 `doc/readme.md`，写明 `config.yaml` 完整字段表与依赖目录约定；平台支持矩阵。
24. Windows 完整支持（或在 `main` 显式拒绝并给出清晰提示）。

### 4.5 建议顺手做的卫生项

- `.gitignore` 的 `.*` 会忽略所有点文件（包括 `.gitignore` 自身、未来的 `.github/`）→ 改成 `/target` + 明确列举。
- 加 `rustfmt.toml`、`.editorconfig`；`Cargo.toml` 补 `description/license/rust-version`。
- 中文注释与英文报错混用（如 `cur project exit loop include` 语法不通）→ 统一用户可见文案。

---

## 五、一句话结论

代码骨架（命令分发 → 模块发现 → 依赖图 → 逐模块编译）方向是对的，但**"发现/配置/编译/链接"四层各自对路径和名字有一套假设**，彼此不一致，导致依赖、include、库名、构建模式这些边界全部漏。建议按 **S0 止血（1 天）→ S1 正确性（1–2 天）** 先把 `hk new` → `hk build` → `hk run` 与跨模块依赖打通并加端到端测试，再做 S2 分层重构（顺带丢掉 `cc`、`petgraph` 两个依赖），最后在 S3 上补增量/并行/C++ 这些真正提升体验的能力。

---

## 六、v0.2 修复对照

| 原编号 | 问题 | 0.2 的处理 |
| --- | --- | --- |
| P0-1 | @@hk new@@ 写死 @@name: app@@ | 未显式配置 name 时取目录名，@@hk new@@ 生成 @@name: <模块名>@@，并附带示例源码与 .gitignore |
| P0-2 | 空 @@src/@@ 也打包静态库导致失败 | 源码为空则不产生归档步骤；@@hk new@@ 后可开箱构建 |
| P0-3 | @@--mode release@@ 不构建、mode 传不到编译器 | mode 改为 profiles 名，@@-m@@ 直接决定 cflags/ldflags/strip/post |
| P0-4 | 依赖模块按模块内部路径解析 | @@Project::resolve_dep@@：模块名 / 目录名 / 相对路径，指向同级目录 |
| P0-5 | include 目录收的是文件 | 收目录及其所有子目录（@@-I@@） |
| P0-6 | 源码不递归、不按扩展名过滤 | 递归扫描 + 扩展名白名单 + 稳定排序 |
| P0-7 | 库名两端不一致、rename 覆盖失败 | @@Naming@@ 统一命名（工具链/目标可覆盖），直接输出到目标路径 |
| P0-8 | @@\\?\@@ verbatim 路径交给外部工具 | @@util::normalize@@ 剥离前缀 |
| P0-9 | 依赖解析不到被静默忽略 | 解析失败直接报错，并带模块名上下文 |
| P0-10 | 模块重名静默覆盖 | 发现阶段检测重名并报错 |
| P0-11 | 配置字段无默认值、未知字段静默 | 全字段默认值 + @@deny_unknown_fields@@（写错 key 立即报错） |
| P0-12 | 库用 cc crate、与 bin 编译参数不一致 | 弃用 cc crate，统一 @@cc -c@@ + @@ar rcs@@ 后端，lib/bin 共用同一套参数生成 |
| P1-1/2 | 三套并行数据结构、图只能走一遍、死字段 | 合并为 @@Project@@（IndexMap）+ 一次性拓扑分层 |
| P1-3 | detect_env 靠试错、模块被加载两次 | 以 @@hk.yaml@@ / @@config.yaml@@ 显式判定工程根，模块只加载一次 |
| P1-4 | Box<dyn Error> + 字符串错误 | @@HkError@@ 上下文链，错误信息含模块与阶段 |
| P1-5 | release 下连错误都不打印 | 自带分级日志，错误始终输出，@@-v/-vv@@ 提高详细度 |
| P1-6 | 无增量、object 目录混用 | @@-MMD -MP@@ 依赖文件 + 命令行签名；obj 按源码相对路径分目录 |
| P1-7 | 只链接直接依赖、顺序按书写顺序 | 传递闭包 + 拓扑逆序 + @@--start-group@@ |
| P1-8 | read_dir 顺序不可复现 | 全部排序 |
| P1-9 | ModFile 513 行、职责过载 | 拆成 config / project / build（flags、steps、编排）四层 |
| P1-10 | @@hk new ../../x@@ 可越界 | 校验模块名；源码 glob 禁止 @@..@@ 与绝对路径 |
| P1-11 | 命令构造重复两遍 | 统一 @@build/flags.rs@@ |
| P1-12 | mode 不落地 | 见 P0-3 |
| P2 | C++ / 多目标 / 并行 / 共享库 / 缓存 | 已支持 C/C++ 混合、分层并行 @@-j@@、static-lib/shared-lib/object/header-only、@@--print-plan@@；仍未做：install、ccache、烧录(flash) |

### 新增能力（对应本次需求）

1. **交叉编译**：@@hk.yaml@@ 的 @@targets@@ 声明工具链 + 架构参数 + specs + sysroot + 链接脚本 + 入口符号 + 命名约定；
   @@--print-plan@@ 可在没有交叉工具链的机器上核对完整命令。
2. **MCU 专用**：内置 @@cortex-m0/m3/m4/m4f/m7@@、@@riscv32@@、@@esp32@@ 目标与 @@mcu-debug/mcu-release/mcu-size@@ 配置档；
   @@-Os@@ + @@-ffunction-sections -fdata-sections@@ + @@-Wl,--gc-sections@@ + nano/nosys specs；
   链接后自动 objcopy 出 @@.bin@@/@@.hex@@、@@-Map@@、@@size@@、@@objdump -d@@ 清单；
   解析 @@--print-memory-usage@@ 输出并打印 Flash/RAM 占用（超出容量会标记）；
   @@runnable: false@@ 让 @@hk run@@ 拒绝执行 MCU 产物。
3. **编译器配置全部来自 YAML**：工具内嵌 @@src/defaults/hk.yaml@@ 作为基线，工程 @@hk.yaml@@ 覆盖；
   @@hk init@@ 导出完整配置，@@hk config --target X@@ 查看某目标解析结果；Rust 代码里没有任何编译器名或编译参数。
4. **模块级 YAML 覆盖常见场景**：executable/static-lib/shared-lib/object/header-only、全局宏与 undefine、
   多源 glob 与排除、per-file 参数、按目标覆盖、@@deps.mods/system/static/shared/lib_dirs@@、
   @@rpath/whole_archive/extra_objects/linker_script/entry/nostdlib@@、@@public@@ 接口传播。

