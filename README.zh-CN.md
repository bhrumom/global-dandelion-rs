# Global Dandelion Rust CLI

> 当前是原生 Rust CLI 的首个可运行核心版本，不是 Python 套壳，也不代表原程序所有功能已经迁移完成。

按照仓库已有 Spec 的顺序，先打通：

```text
配置 -> Supervisor -> 只读文件扫描 -> 有界队列 -> 分块编码
     -> 限速与流量预算 -> 真实 UDP 回环发送 -> 接收校验
```

## 已实现

可执行文件名为 `dandelion`。支持标准子命令、帮助、版本、Shell 补全、JSON 结果、结构化日志、配置验证、只读扫描、可取消任务、严格数据包校验与本地自检。

支持 Linux x86_64 和 ARM64；当前 GNU/Linux 产物在 Ubuntu 24.04 上构建验收，需要兼容的 glibc 用户空间，不是静态 musl 或 macOS 二进制。文件安全打开需要 Linux 5.6+ 的 `openat2`。无需 root，不会修改系统时间、修改网络、重命名源文件或自动扫描所有硬盘。

```sh
dandelion --help
dandelion doctor
dandelion self-test --json
dandelion config init
dandelion --config examples/loopback.toml config check
dandelion --config examples/loopback.toml run --json
dandelion run --source /path/to/data --dry-run --json
dandelion run --source /path/to/data --traffic-limit-bytes 67108864 --json
dandelion completions bash
```

`config init` 只在 stdout 输出默认配置，不会覆盖文件。`run` 执行一轮有限扫描后退出。实际发送只发生在本机 `127.0.0.1`，不会向公网或局域网设备发送。接收方校验数据包，但不落盘重建文件。

## 架构与默认约束

8 个实际模块：`app`、`core`、`config`、`scanner`、`packet`、`pacing`、`transport`、`platform-linux`。核心域不依赖 Tokio、CLI、Linux 实现或 socket。

默认队列容量 32 个数据块；有效载荷 1,152 字节；速率 1 MiB/s；每次运行线上流量上限 64 MiB；源文件读取预算 256 MiB。数据包使用版本化大端编码和 BLAKE3 校验，但这不是加密或身份认证。

配置优先级：默认值 < TOML < 环境变量 < CLI。例如：

```sh
export DANDELION__RUNTIME__QUEUE_CAPACITY=16
export DANDELION__PACING__BYTES_PER_SECOND=524288
```

日志写 stderr，结果写 stdout。退出码：`0` 成功，`1` 运行/文件错误，`2` 配置/参数错误，`4` 校验失败，`75` 预算耗尽，`130` 取消。

## 获取安装产物

使用 GitHub Actions 中 **验收已通过** 的构建产物。先解压下载的 GitHub ZIP，再校验并解压其中的 tar.gz；tar 包保留可执行权限。产物包含二进制、依赖锁文件、提交和工具链信息、SHA-256 校验和验收记录。ARM64 使用对应的 aarch64 文件名。不要把校验和当作签名信任链。

```sh
sha256sum --check dandelion-x86_64-unknown-linux-gnu.tar.gz.sha256
mkdir dandelion-bin
tar -xzf dandelion-x86_64-unknown-linux-gnu.tar.gz -C dandelion-bin
cd dandelion-bin
sha256sum --check SHA256SUMS
./dandelion self-test --json
```

编译、测试和打包放在 GitHub Actions 上执行，不在空间紧张的常用设备上重型构建。任何写入前都要预估峰值用量，保留至少 `max(1 GiB, 磁盘容量的 5%)` 可用空间。

## 未完成范围

LAN/VPS/授权 WAN 传输、自动硬盘发现与热插拔、持久化状态/恢复、热点、TUI、自动更新、旧协议适配和 Python 对比基准仍在迁移清单中。不支持的模式会明确报错，不会用空实现返回成功。当前没有宣称比 Python 快多少倍，也不应直接退役原 Python 全功能程序。

详细设计见 [Spec](docs/specs/global-dandelion-rust-architecture.md)、[ADR](docs/adr/0001-first-vertical-slice.md)、[迁移清单](docs/migration/python-parity-ledger.md)与[English README](README.md)。
