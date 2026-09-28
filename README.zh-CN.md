# Global Dandelion 原生 Rust CLI

本仓库以原生 Rust 重构 `global_dandelion.py`，可执行文件为 `dandelion`，不依赖 Python 运行时。
**Spec 已整体替换为 v2，不再停留在第一阶段回环示例。**旧版仅在 `docs/history/architecture-v1.md` 归档。

当前代码覆盖原脚本的主要功能领域，迁移清单保留全部 37 项职责：34 项有对应实现，3 项危险旧默认行为明确移除。
“代码实现”和“完整替代验收”分开记录：物理无线热点、真实硬盘插拔、生产签名信任与完整对照切换仍需独立证据，不能用编译通过替代。

## 架构

采用模块化单体、向内依赖、能力接口、有界数据流水线和统一 Supervisor。
13 个实际实现的 crate：`app`、`core`、`config`、`scanner`、`packet`、`pacing`、`transport`、`platform-linux`、`state`、`tui`、`updater`、`hotspot`、`compat`。

```text
CLI / 配置 -> Supervisor / 持续运行控制
                  |
安全文件句柄 -> 扫描与读取 -> 有界队列 -> 编码 -> 限速与预算 -> 传输 -> 校验确认
                  |                                      |
              文件级检查点                            目标健康与统计
                  |                                      |
              原子持久化                         只读快照 / TUI
```

核心域不依赖 Tokio、Linux、CLI 或具体 socket。应用层注入文件、检查点、进程和磁盘写入策略接口；扫描器不直接依赖 Linux 或持久化实现。没有用空 crate、假兼容层或 no-op 命令冒充功能。

## 使用入口

```sh
dandelion --help
dandelion self-test --json
dandelion config init
dandelion --config examples/service.toml config check
dandelion run --source /srv/data --dry-run --json
dandelion run --source /srv/data --repeat 3 --interval-ms 1000 --json
dandelion --config examples/service.toml run --resume --json
dandelion run --source /srv/data --tui
dandelion scan metadata --source /srv/data --json
dandelion scan read --source /srv/data --json
dandelion platform inspect
dandelion platform watch --samples 10 --interval-ms 1000
dandelion platform time
dandelion state inspect --directory .dandelion-state
dandelion state import --directory .dandelion-state --file traffic_state.json
dandelion completions bash
```

默认是有限的一轮本机回环任务。`--watch` 必须显式启用，仍受流量额度约束；每轮扫描共用限速器，不能重新获得额外突发额度。
元数据扫描不读取文件内容；`scan read` 只读文件并校验，不打开传输 socket。正常扫描和发送不改名、不修改源文件内容。

## 网络、状态与恢复

LAN、私有 VPS 和授权 WAN 使用双向 TLS 1.3，验证 CA、客户端证书、服务器名称与协议。目标必须是明确授权的数值地址；WAN 另外要求开启开关并列入精确地址允许列表。发现局域网设备或挂载盘，不等于授权发送。

```sh
dandelion receive --bind 127.0.0.1:20000 --ca-file credentials/ca.pem \
  --certificate-file credentials/server.pem --private-key-file credentials/server.key --json
dandelion --config examples/tls-client.toml run --json
dandelion --config examples/tls-client.toml platform time --peer
```

计量器位于 TLS 下层，握手、加密记录、确认消息和控制消息都受限。统计的是 socket 接受的字节，不是物理链路全部流量。接收确认表示经过认证的内存校验，**不是远端文件已经持久化**。

持久化默认关闭；启用后使用私有目录、单写者锁、临时文件同步、原子替换和目录同步。网络开始前先持久化本轮预算预留，正常结算才退回确认未使用的额度；异常或崩溃保守保留预留，损坏状态不会悄悄清零。额度是生命周期额度，不自动按月重置。

`--resume` 是**文件级恢复**：必须启用状态，只执行一轮，最多记录 4096 个确认文件。目标策略或证书变化、文件身份或内容元数据变化都会失效。未完成文件整文件重读；突然断电可能重放上次持久化后的工作，不承诺字节级续传或恰好一次远端存储。

## 热点、更新和旧行为兼容

`hotspot plan` 只展示计划。`hotspot apply/stop` 需要显式 `--confirm`、现有系统授权和私有日志目录；仅操作本程序拥有的临时 NetworkManager 配置，失败时尝试恢复。密码从私有文件读取，不进入命令参数；不会隐式提权或修改无线法规设置。真实无线硬件验收仍未完成。

`update check/stage` 验证独立信任的 Ed25519 公钥、签名、版本、架构、过期时间、SHA-256 和文件长度。只暂存，不直接执行或覆盖当前二进制。提供可选 systemd 服务和周期检查模板，**不会自动安装、启用或改动用户系统**。生产签名密钥和激活/回滚需要部署验收。

`compat` 提供旧 MD5 数据包检查、国家 CSV 元数据查询、显式链接练习与日志恢复。原脚本的改名效果实际作用于隐藏硬链接和符号链接，而非原文件；新实现只在私有工作目录中操作有限的自有链接。它会改变 inode 元数据，不能与源文件扫描并行使用。不会按文件名猜测并删除陌生文件。

## 验收与产物

编译、测试、审计和打包全部放在 GitHub Actions 的临时执行器上。包括双架构测试、真实 UDP/TLS 正反例、文件恢复、真实 PTY 终端恢复、签名篡改拒绝、安装包解压和权限校验。必须查看当前提交对应的运行结果，不能引用旧 HEAD 的绿色状态。

产物为 Ubuntu 24.04 上构建的 GNU/Linux x86_64 和 ARM64 二进制，需要兼容 glibc 和 Linux 5.6+ 安全文件访问支持，不是 macOS 安装包。产物包含校验和、构建信息、第三方许可证、验收 JSON 和基准原始数据。

```sh
sha256sum --check dandelion-x86_64-unknown-linux-gnu.tar.gz.sha256
mkdir dandelion-bin
tar -xzf dandelion-x86_64-unknown-linux-gnu.tar.gz -C dandelion-bin
cd dandelion-bin
sha256sum --check SHA256SUMS
./dandelion self-test --json
```

Python/Rust 对照基准使用相同的纯旧协议编码负载，不运行原脚本带系统副作用的主入口；这些结果不能冒充完整程序、WAN 或硬盘性能。完整替代验收还需要物理设备、生产信任链和受控全流程测量。

[Spec v2](docs/specs/global-dandelion-rust-architecture.md) · [迁移清单](docs/migration/python-parity-ledger.md) · [验收门](docs/acceptance-gates.json) · [操作说明](docs/operations.md) · [设计决策](docs/adr/) · [开发记录](docs/development/2026-09-28-complete-migration.md)
