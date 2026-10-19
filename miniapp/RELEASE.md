# 发布与托管

包的**字节**托管在 GitHub Releases，**信任**仍由 Fabushi Marketplace 签发。两者分离，任何一方单独失守都不足以让宿主装上一个被替换的小程序。

## 为什么这样拆

宿主安装一个 fabapp 要过三道校验：

1. `verify_install_proof` — 用二进制里 pin 住的 Ed25519 公钥验证 release 签名载荷；
2. `verify_catalog_bundle` — 用签名载荷里的公钥验证包内 `manifest.json` 的签名；
3. 下载后再核 `sha256` 与 `sizeBytes`。

因此 GitHub 只可能"藏起"字节（下载失败），不可能"换掉"字节（摘要不符）。签名私钥从未离开 registry，公开仓库里也没有任何可签发凭据。

## 流水线

```
global-dandelion-rs CI          产出 unsigned 资源 + manifest.unsigned.json
        │                       （miniapp-ci.yml，不发布）
        ▼
backend delivery job            调 /v1/extensions/review/fabapp/sign
        │                       用 production 私钥签 manifest，按 pin 住的公钥自检
        ▼
signed-delivery/*.fabapp        最终字节，digest 由签名决定
        │
        ▼
publish-github-release.py       上传到公开仓库 Release → 确认 asset 大小
        │                       把 (digest, sizeBytes, url) 登记回 registry
        ▼
review approve                  签名载荷获得 url 字段 → 宿主优先从 GitHub 取字节
```

`publish-github-release.py` 只在 `DANDELION_RELEASE_TOKEN` 存在时运行；缺失时签名 job 照旧产出 `signed-delivery/`，不影响既有交付。

## 签名载荷里的 url

签名载荷的 `artifacts[]` 新增可选字段 `url`：

```json
{"artifactId":"universal-ui","digest":"…","sizeBytes":453093,
 "platform":"all","architecture":"any","format":"fabapp",
 "url":"https://github.com/bhrumom/global-dandelion-rs/releases/download/v0.2.0/global-dandelion-0.2.0.fabapp"}
```

- 字段缺失时载荷字节与旧版完全一致，registry 托管的历史 release 签名不受影响；
- 字段存在时宿主只接受 `https`、默认端口、无凭据、无 fragment，且主机必须在 `github.com` / `objects.githubusercontent.com` / `release-assets.githubusercontent.com` 之内；
- 镜像 404 或网络失败会回落到 registry 路由；摘要或大小不符则直接失败，不回落——那是替换信号，不是重试理由；
- 镜像请求允许重定向（GitHub Release asset 会 302 到 CDN），但每一跳都要重新过主机白名单，链长上限 5。

## 校验已下载的包

```bash
shasum -a 256 global-dandelion-0.2.0.fabapp
```

与 `signed-delivery/provenance.json` 的 `packageSha256`、以及 registry 安装证明里的 `digest` 三方一致即可。包内 `manifest.json` 的 `signature.keyId` 必须是 `fabushi-marketplace-managed-20261013-1`。

## 需要的 secret

| 名称 | 所在仓库 | 用途 |
| --- | --- | --- |
| `FABUSHI_MARKETPLACE_REVIEW_TOKEN` | `bhrumom/fabushi-backend` | 调签名服务、登记外部制品 |
| `FABUSHI_MARKETPLACE_SIGNING_PRIVATE_KEY` | production 运行时环境 | manifest 签名（从不下发到 Actions 日志） |
| `DANDELION_RELEASE_TOKEN` | `bhrumom/fabushi-backend` | 对公开仓库写 Release；建议 fine-grained PAT，仅 `contents: write` 于 `global-dandelion-rs` |

## 失败时的行为

| 阶段失败 | 结果 |
| --- | --- |
| 准备包摘要不符 | 拒绝签名，不产出任何包 |
| 签名自检失败 | 拒绝产出，`signed-delivery/` 为空 |
| Release 已存在同名 asset 但大小不同 | 报错退出，绝不覆盖已发布字节 |
| registry 登记失败 | 字节已上传但无签名 url，宿主回落 registry 路由（该 release 仍不可安装，因为 registry 无对应字节） |

## 运维前提：registry 必须已部署最新代码

`release-import` 路由（`POST /v1/extensions/review/releases/:kind/:ns/:name/:version/import`）
不存在时，交付会在 `phase=release-import` 报 `404`。部署走 `marketplace-node-delivery.yml`：

```bash
gh workflow run marketplace-node-delivery.yml -R bhrumom/fabushi-backend --ref main
```

该 workflow 的接收端有一道**磁盘保留检查**：`/opt/dacheng-ai` 剩余空间减去新运行时
展开体积和 SQLite 大小后，必须仍大于 `max(1GB, 总容量/20)`，否则拒绝部署并只打印
`Restricted deployment failed: ValueError`（接收端刻意不回显具体原因）。历史 release 与
backup 不会自动清理，攒多了就会撞上这道检查。清理（在 registry 主机上，root）：

```bash
ls -lt /opt/dacheng-ai/releases/          # 最新的那个是当前在跑的，别删
ls -lt /opt/dacheng-ai/backups/
sudo rm -rf /opt/dacheng-ai/releases/<旧 sha> /opt/dacheng-ai/backups/<旧备份>
```

当前生效的 release 由
`/etc/systemd/system/dacheng-ai-backend.service.d/zz-fabushi-canonical-release.conf`
里的 `WorkingDirectory` 决定，删之前先确认不是它。

## 发布 0.2.0 的具体步骤

### 1. 取准备包

```bash
gh run download <offline-miniapp-run-id> -R bhrumom/global-dandelion-rs \
  -n offline-miniapp-preparation-<sha> -D prep
```

`prep/` 里有 `global-dandelion-UNSIGNED.zip`、`global-dandelion-UNSIGNED-android.zip`、
`global-dandelion-0.2.0.html` 与 `miniapp-resources/`。

### 2. 造 preparation.zip 并挂到 backend 的 draft release

```bash
python3 fabushi-backend/.github/scripts/make-preparation-archive.py \
  --source prep --output preparation.zip

gh release create dandelion-preparation-<sha8> preparation.zip \
  -R bhrumom/fabushi-backend --draft --title "preparation <sha8>" --notes "reviewed preparation"
```

必须用这个脚本，**不要用 `zip -r`**：`zip -r` 会把目录本身写成成员（`miniapp-resources/`），
而交付校验要求每个成员路径没有空段，否则在 `phase=preparation-integrity` 就直接失败，
根本走不到签名那一步。脚本只写文件条目，并打印 `preparation_digest`。

`--draft` 保证它对公众不可见；`gh release download` 仍能读到。

### 3. 跑签名 workflow

```bash
gh workflow run global-dandelion-package-delivery.yml -R bhrumom/fabushi-backend \
  -f version=0.2.0 \
  -f preparation_release=dandelion-preparation-<sha8> \
  -f source_commit=<40 位 sha> \
  -f preparation_artifact_id=<run 里的 artifact id> \
  -f preparation_digest=<preparation.zip 的 sha256>
```

五个 input 必须与第 1、2 步的实际值一致，否则脚本在摘要校验处直接失败。

### 4. 让字节落到公开仓库

workflow 的最后一步自动完成，不需要额外配置：

- **有 `DANDELION_RELEASE_TOKEN`**（跨仓 PAT）：字节进 `bhrumom/global-dandelion-rs`。
- **没有**：字节进 `bhrumom/fabushi-backend`（job 自己的 `github.token`，该仓库同样是公开的）。

两种落点都只是"存字节"：签名载荷里的 `url` 指向哪，宿主就去哪取，取回后照样用
`sha256` + `sizeBytes` 复核，不符即拒绝安装。信任根始终在 registry。

同一步还会做完 registry 侧的三件事，否则包签好了也没人装得上：

| 阶段 | 作用 | 幂等性 |
|---|---|---|
| `release-import` | 建目录记录（`pending_review`），descriptor 从 `source_commit` 那个提交取 | 同 descriptor 重复导入返回原记录；不同 descriptor 是冲突 |
| `registry-binding` | 绑定 `(digest, sizeBytes, url)` | 同 artifactId 换 url 才改绑，换 digest 直接拒绝 |
| `scan-record` | 记录扫描结论（审批门禁要求） | 结论不可改写 |

descriptor 里的 `release.bundleDigest` 由脚本按实际字节填入，不读人工维护的字段——
元数据永远不可能和包不一致。

需要换落点时手动跑同一条命令：

```bash
gh run download <delivery-run-id> -R bhrumom/fabushi-backend \
  -n global-dandelion-signed-delivery-<sha> -D signed-delivery

DANDELION_SOURCE_COMMIT=<40 位 sha> \
DANDELION_RELEASE_TOKEN=<pat> FABUSHI_MARKETPLACE_REVIEW_TOKEN=<token> \
  python3 fabushi-backend/.github/scripts/publish-github-release.py
```

### 5. 审批（可选，默认不做）

审批是**独立决策**，默认不执行：workflow 只把 release 推到 `pending_review`。
要让宿主真的能装，得显式加 `-f approve=true` 重跑，或者单独调用审批路由：

```bash
gh workflow run global-dandelion-package-delivery.yml -R bhrumom/fabushi-backend \
  -f version=0.2.0 -f approve=true \
  -f preparation_release=dandelion-preparation-<sha8> \
  -f source_commit=<40 位 sha> \
  -f preparation_artifact_id=<artifact id> \
  -f preparation_digest=<sha256>
```

审批必须在**登记之后**：url 是 artifact 记录的字段，而带 url 的 release 签名是在审批那一刻
才生成的。顺序反了只能重发一个版本，因为已有签名不可改写。

审批通过后，`GET /v1/extensions/install/{kind}/{ns}/{name}/{version}` 返回的签名载荷里
`artifacts[0].url` 就是 GitHub 地址，宿主随后优先从那里取字节。

### 验证

```bash
gh release view v0.2.0 -R bhrumom/fabushi-backend
shasum -a 256 global-dandelion-0.2.0.fabapp   # 与 provenance.json 的 packageSha256 一致

curl -s https://api.ombhrum.com/v1/extensions/install/web-miniapp/bhrumom/global-dandelion/0.2.0 \
  | python3 -c 'import json,sys; p=json.load(sys.stdin); print(json.loads(p["releaseSignature"]["payload"])["artifacts"])'
```
