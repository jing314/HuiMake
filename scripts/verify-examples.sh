#!/usr/bin/env bash
# 逐个验证 examples/ 下的示范工程，并打印 PASS/FAIL 汇总表。
#
#   01-single-module / 02-multi-module : 真实编译并运行（需要本机有 gcc）
#   03-mcu-stm32   / 04-mcu-gd32       : 执行 --print-plan 并检查关键交叉编译参数（不需要交叉工具链）
#
# 每个示例都会复制到临时目录再构建，不会污染 examples/。
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"
examples="$root/examples"

hk="${HK:-}"
if [ -z "$hk" ]; then
  if [ -x "$root/target/debug/hk" ]; then
    hk="$root/target/debug/hk"
  elif [ -x "$root/target/release/hk" ]; then
    hk="$root/target/release/hk"
  else
    echo "未找到 hk，先执行 cargo build ..."
    cargo build >/dev/null
    hk="$root/target/debug/hk"
  fi
fi

has_gcc=0
if command -v gcc >/dev/null 2>&1; then has_gcc=1; fi

echo "hk  : $hk"
if [ "$has_gcc" -eq 1 ]; then echo "gcc : 已找到"; else echo "gcc : 未找到（本机示例会被跳过）"; fi
echo

names=(01-single-module 02-multi-module 03-mcu-stm32 04-mcu-gd32)
workdirs=(01-single-module/hello 02-multi-module 03-mcu-stm32 04-mcu-gd32)
modes=(run run plan plan)
argss=(
  "build"
  "build"
  "build -t stm32f407 -m mcu-release --print-plan"
  "build -t gd32f303 -m gd32-release --print-plan"
)
expectss=(
  "构建完成"
  "libcore.a|libsensor.a"
  "arm-none-eabi-gcc|-mcpu=cortex-m4|-specs=nano.specs|STM32F407VGTx_FLASH.ld|-Wl,--print-memory-usage|startup_stm32f407|libbsp.a|libhal.a|firmware.elf|arm-none-eabi-objcopy -O binary|arm-none-eabi-objcopy -O ihex|arm-none-eabi-size"
  "arm-none-eabi-gcc|-mcpu=cortex-m4|-DGD32F30X_HD|-O2|GD32F303CCT6_FLASH.ld|startup_gd32f303|libgd32_driver.a|firmware.elf|arm-none-eabi-objcopy -O binary"
)
runexes=(01-single-module/hello/build/bin/hello 02-multi-module/app/build/bin/app "" "")
runexpects=("HELLO FROM HK" "sensor=43 core=2 sqrt=4" "" "")

fail=0
printf "%-18s %-6s %-6s %s\n" "示例" "模式" "结果" "说明"

for i in 0 1 2 3; do
  name="${names[$i]}"
  workdir="${workdirs[$i]}"
  mode="${modes[$i]}"
  args="${argss[$i]}"
  expects="${expectss[$i]}"
  runexe="${runexes[$i]}"
  runexpect="${runexpects[$i]}"

  if [ "$mode" = "run" ] && [ "$has_gcc" -eq 0 ]; then
    printf "%-18s %-6s %-6s %s\n" "$name" "$mode" "SKIP" "本机没有 gcc"
    continue
  fi

  tmp="$(mktemp -d)"
  cp -R "$examples/$name" "$tmp/$name"
  work="$tmp/$workdir"
  read -r -a argv <<< "$args"

  out="$(cd "$work" && "$hk" "${argv[@]}" 2>&1)" && code=0 || code=$?

  note=""
  status="FAIL"
  if [ "$code" -ne 0 ]; then
    note="构建失败，退出码 $code"
  else
    missing=""
    IFS='|' read -r -a needles <<< "$expects"
    for needle in "${needles[@]}"; do
      case "$out" in
        *"$needle"*) ;;
        *) missing="$missing $needle" ;;
      esac
    done
    if [ -n "$missing" ]; then
      note="缺少标记:$missing"
    elif [ -n "$runexpect" ]; then
      exe="$work/$runexe"
      if [ ! -x "$exe" ]; then
        note="没有找到可执行产物 $runexe"
      elif programOut="$("$exe" 2>&1)" && case "$programOut" in *"$runexpect"*) true ;; *) false ;; esac; then
        status="PASS"
        note="构建 + 运行通过"
      else
        note="运行输出不符: $programOut"
      fi
    else
      status="PASS"
      note="参数检查通过"
    fi
  fi

  printf "%-18s %-6s %-6s %s\n" "$name" "$mode" "$status" "$note"
  if [ "$status" = "FAIL" ]; then fail=$((fail + 1)); fi

  if [ "${KEEP:-0}" != "1" ]; then rm -rf "$tmp"; else echo "  （保留临时目录 $tmp）"; fi
done

echo
if [ "$fail" -gt 0 ]; then
  echo "有 $fail 个示范工程验证失败"
  exit 1
fi
echo "全部示范工程验证通过"
