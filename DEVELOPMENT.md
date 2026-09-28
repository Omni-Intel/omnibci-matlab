# 源码构建与开发验证

本文件适用于包含 Rust 源码、SDK 子模块及测试脚本的 Git 仓库。预编译 ZIP 用户请使用 README.md 中的安装与自检步骤，无需执行本文命令。

## 构建

需要对应平台的 MATLAB、Rust stable 和本机编译工具链。Apple Silicon 原生 MATLAB 从 R2023b 开始提供，但当前 Mac 预编译包的最低版本为 R2025a；更早版本上的源码构建不在当前验证范围内。获取仓库时初始化 SDK 子模块：

```sh
git clone --recurse-submodules git@github.com:Omni-Intel/omnibci-matlab.git
```

在 MATLAB 中运行：

```matlab
cd('D:/workspace/omnibci-matlab')
build_omnibci
addpath(fullfile(pwd, 'matlab'))
addpath(fullfile(pwd, 'tests'))
test_offline
```

`build_omnibci` 使用 `cargo build --offline --locked --release --features mex`；首次构建如缺少 Cargo 依赖，请先在可联网环境运行 `cargo fetch --locked`。生成的 MEX 位于 `matlab/+omnibci/private`，不会被 Git 提交。修改 Rust 桥接代码后须重新运行构建，并在 MATLAB 中先 `clear mex`，再覆盖已有 MEX 文件。

## 验证

```matlab
addpath('D:/workspace/omnibci-matlab/matlab')
addpath('D:/workspace/omnibci-matlab/tests')
test_offline
```

Rust 可运行 `cargo test --locked` 和 `cargo clippy --locked -- -D warnings`。离线测试覆盖 MATLAB 到 Rust 的 MEX 调用、帧 CRC、符号扩展、增益缩放与无效 endpoint。本机已用目标硬件和固件完成 USB 与 BLE 的 3 秒采集验证；长时采集和停止尾包仍需验收。

本机连接板子后可运行 `test_hardware("usb")` 或 `test_hardware("ble")`。USB 脚本使用 COM8；BLE 从扫描结果中选择名为 `OmniBCI` 的设备。测试会回写当前配置、连续读取 3 秒，并检查数据形状、数量与停止后的状态。运行前应关闭其他占用设备的程序。

安装诊断可在仓库根目录用独立 MATLAB 进程验证，避免已加载的另一份 `omnibci.Board` 干扰隔离测试：

```sh
matlab -batch "addpath(fullfile(pwd, tests)); test_installation"
```
