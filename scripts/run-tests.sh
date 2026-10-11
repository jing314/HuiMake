#!/usr/bin/env bash
# 运行 HuiMake 的全部测试：单元测试 + 端到端测试 + 示范工程测试。
#
# 用法：
#   bash scripts/run-tests.sh                  # 全部测试
#   bash scripts/run-tests.sh example_tests     # 只跑名字含该字符串的用例
#   bash scripts/run-tests.sh --unit-only       # 只跑二进制内的单元测试
#   bash scripts/run-tests.sh --skip-clippy     # 跳过 clippy
set -euo pipefail
cd "$(dirname "$0")/.."

filter=""
unit_only=0
skip_clippy=0

for arg in "$@"; do
  case "$arg" in
    --unit-only) unit_only=1 ;;
    --skip-clippy) skip_clippy=1 ;;
    -h|--help)
      sed -n '2,10p' "$0"
      exit 0
      ;;
    *) filter="$arg" ;;
  esac
done

echo "== 环境 =="
cargo --version
rustc --version
if command -v gcc >/dev/null 2>&1; then
  echo "gcc: $(gcc --version | head -n1)"
else
  echo "gcc: 未找到（依赖 gcc 的端到端用例会被跳过）"
fi

if [ "$skip_clippy" -eq 0 ]; then
  echo
  echo "== clippy =="
  cargo clippy --all-targets -- -D warnings
fi

echo
echo "== cargo test =="
args=(test)
if [ "$unit_only" -eq 1 ]; then
  args+=(--bin hk)
fi
if [ -n "$filter" ]; then
  args+=(-- "$filter")
fi
echo "cargo ${args[*]}"
cargo "${args[@]}"

echo
echo "全部测试通过"
