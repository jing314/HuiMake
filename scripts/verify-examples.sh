#!/usr/bin/env bash
# Verify every example project under examples/ and print a PASS/FAIL table.
#
#   01-single-module / 02-multi-module : really compile and run (needs gcc)
#   03-mcu-stm32   / 04-mcu-gd32       : run --print-plan and check the cross flags (no toolchain)
#
# Every example is copied to a temp directory first, so examples/ stays clean.
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
    echo "hk not found, running cargo build ..."
    cargo build >/dev/null
    hk="$root/target/debug/hk"
  fi
fi

has_gcc=0
if command -v gcc >/dev/null 2>&1; then has_gcc=1; fi

echo "hk  : $hk"
if [ "$has_gcc" -eq 1 ]; then echo "gcc : found"; else echo "gcc : not found (host examples will be skipped)"; fi
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
  "Build finished"
  "libcore.a|libsensor.a"
  "arm-none-eabi-gcc|-mcpu=cortex-m4|-specs=nano.specs|STM32F407VGTx_FLASH.ld|-Wl,--print-memory-usage|startup_stm32f407|libbsp.a|libhal.a|firmware.elf|arm-none-eabi-objcopy -O binary|arm-none-eabi-objcopy -O ihex|arm-none-eabi-size"
  "arm-none-eabi-gcc|-mcpu=cortex-m4|-DGD32F30X_HD|-O2|GD32F303CCT6_FLASH.ld|startup_gd32f303|libgd32_driver.a|firmware.elf|arm-none-eabi-objcopy -O binary"
)
runexes=(01-single-module/hello/build/bin/hello 02-multi-module/app/build/bin/app "" "")
runexpects=("HELLO FROM HK" "sensor=43 core=2 sqrt=4" "" "")

fail=0
printf "%-18s %-6s %-6s %s\n" "example" "mode" "result" "note"

for i in 0 1 2 3; do
  name="${names[$i]}"
  workdir="${workdirs[$i]}"
  mode="${modes[$i]}"
  args="${argss[$i]}"
  expects="${expectss[$i]}"
  runexe="${runexes[$i]}"
  runexpect="${runexpects[$i]}"

  if [ "$mode" = "run" ] && [ "$has_gcc" -eq 0 ]; then
    printf "%-18s %-6s %-6s %s\n" "$name" "$mode" "SKIP" "gcc is not available"
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
    note="build failed with exit code $code"
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
      note="missing markers:$missing"
    elif [ -n "$runexpect" ]; then
      exe="$work/$runexe"
      if [ ! -x "$exe" ]; then
        note="no executable artifact found: $runexe"
      elif programOut="$("$exe" 2>&1)" && case "$programOut" in *"$runexpect"*) true ;; *) false ;; esac; then
        status="PASS"
        note="built and ran successfully"
      else
        note="unexpected program output: $programOut"
      fi
    else
      status="PASS"
      note="flag checks passed"
    fi
  fi

  printf "%-18s %-6s %-6s %s\n" "$name" "$mode" "$status" "$note"
  if [ "$status" = "FAIL" ]; then fail=$((fail + 1)); fi

  if [ "${KEEP:-0}" != "1" ]; then rm -rf "$tmp"; else echo "  (keeping temp directory $tmp)"; fi
done

echo
if [ "$fail" -gt 0 ]; then
  echo "$fail example(s) failed"
  exit 1
fi
echo "All example projects verified"
