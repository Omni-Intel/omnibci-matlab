# 源码构建与开发验证

## 构建

需要 MATLAB、Rust stable 和本机编译工具链。Rust 直接链接 MATLAB 的 MEX 库，无需 Python 或单独的 C++ MEX 编译器。SDK 由 Git 子模块固定提交。

```sh
git clone --recurse-submodules git@github.com:Omni-Intel/omnibci-matlab.git
cd omnibci-matlab
cargo fetch --locked
```

Linux 构建另需 `libudev-dev`、`libdbus-1-dev` 和 `pkg-config`。各平台构建版本见 [README](README.md#安装)；更早 MATLAB 版本的源码构建未经验证。

在 MATLAB 中切换到仓库根目录，运行：

```matlab
clear mex
build_omnibci
addpath(fullfile(pwd, 'matlab'))
addpath(fullfile(pwd, 'tests'))
test_offline
```

`build_omnibci` 执行 `cargo build --offline --locked --release --features mex`，将 MEX 复制到 `matlab/+omnibci/private`。修改 Rust 桥接代码后重新构建；覆盖已加载的 MEX 前先执行 `clear mex`。构建产物不纳入 Git。

## 验证

在仓库根目录运行 Rust 检查：

```sh
cargo test --locked
cargo clippy --locked -- -D warnings
```

MATLAB 的 `test_offline` 覆盖 MEX 调用、版本查询、空批次、帧 CRC、符号扩展、增益缩放和无效 endpoint 拒绝。

安装诊断须使用独立 MATLAB 进程，避免已加载的 `omnibci.Board` 干扰隔离测试：

```sh
matlab -batch "addpath(fullfile(pwd, 'tests')); test_installation"
```

硬件测试前关闭占用设备的程序，在已添加 `matlab` 和 `tests` 路径的 MATLAB 中运行：

```matlab
test_hardware("usb")  % COM8；使用其他端口时修改 tests/test_hardware.m
test_hardware("ble")  % 扫描名为 OmniBCI 的设备
```

测试回写当前配置、连续读取 3 秒，检查数据形状、数量和停止后的状态。Windows R2023a 的 USB/BLE 短时采集已通过；长时采集、停止尾包及 Linux/macOS 硬件连接尚未验证。

## 发布

`.github/workflows/release.yml` 为四个平台执行 Rust 测试、Clippy、MEX 构建和 ZIP 打包；全部成功后才发布。CI 读取私有 SDK 子模块需要仓库 Secret `SUBMODULES_READ_TOKEN`，不执行 MATLAB 运行测试或设备测试。

- 推送 `vX.Y.Z` 标签触发发布，标签须匹配 `Cargo.toml` 的版本。
- 手动运行时，留空 `release_tag` 只生成 Actions artifact；填写已有版本标签则从该标签重新构建并发布。
- 同名 Release 已存在时，工作流覆盖对应 ZIP。

`scripts/package_release.py` 打包 MATLAB 文件、MEX、示例、README 和许可证；开发文档、Rust 源码及测试不随 ZIP 分发。
