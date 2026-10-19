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
cd prep && zip -qr ../preparation.zip . && cd ..
shasum -a 256 preparation.zip

gh release create dandelion-preparation-<sha8> preparation.zip \
  -R bhrumom/fabushi-backend --draft --title "preparation <sha8>" --notes "reviewed preparation"
```

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

两条路，任选一条：

- **有 `DANDELION_RELEASE_TOKEN`**：workflow 的最后一步自动建 release、传 asset、登记 url。
- **没有**：下载签名产物后手动执行同一条命令。

```bash
gh run download <delivery-run-id> -R bhrumom/fabushi-backend \
  -n global-dandelion-signed-delivery-<sha> -D signed-delivery

DANDELION_RELEASE_TOKEN=<pat> FABUSHI_MARKETPLACE_REVIEW_TOKEN=<token> \
  python3 fabushi-backend/.github/scripts/publish-github-release.py
```

脚本做的事：建/复用 `v0.2.0` release → 确认 asset 大小与签名包一致（不一致绝不覆盖）→
用 review token 把 `(digest, sizeBytes, url)` 登记到 registry。

### 5. 审核并让签名载荷带上 url

登记必须发生在 **release 审核通过之前**：url 是 artifact 记录的字段，而审核通过那一刻才
生成带 url 的 release 签名。顺序反了就只能重发一个版本，因为已有签名不可改写。

```bash
# 提交 + 扫描 + 审核（沿用现有 marketplace review 流程）
```

审核通过后，`GET /v1/extensions/install/{kind}/{ns}/{name}/{version}` 返回的签名载荷里
`artifacts[0].url` 就是 GitHub 地址，宿主随后优先从那里取字节。

### 验证

```bash
gh release view v0.2.0 -R bhrumom/global-dandelion-rs
shasum -a 256 global-dandelion-0.2.0.fabapp   # 与 provenance.json 的 packageSha256 一致
```
