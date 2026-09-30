# Runbook — sdkwork-payment 生产首次上线（host 包路径）

适用：standalone 宿主包交付（Ubuntu 目标机）。容器交付（cloud/k8s）走 `bin/docker-image.sh` + 编排，见 `deploy.md`。

## 0. 前置条件（目标机）

- Ubuntu 22.04+，具备 root 的 ssh 账户（wrapper 不注入 sudo，操作者用 `--host ssh://root@host` 控制身份）。
- PostgreSQL 实例可达；数据库/账号已建（`SDKWORK_DATABASE_*`）。
- 出网或内网可达 PSP 生产网关与（可选）OTLP collector。
- `ca-certificates` 已安装（PSP TLS 依赖）。

## 1. 打包

```bash
bin/apps-package.sh server production --out target/bin-packages
# 产出 target/bin-packages/sdkwork-payment-server-<version>-linux-<arch>.tar.gz (+ .sha256)
```

包内容（PACKAGING_SPEC §5.2/§6）：剥离后的网关二进制（支持 `--help`/`--version`）、`install/install-ubuntu.sh`、systemd 单元、`config/env/payment.env.example`、生成式 `install-manifest.json`（逐文件 sha256）。Linux 构建必须在 Linux 执行（Windows/macOS 开发机产物会诚实标注为对应 OS 且不含安装器）。

## 2. 目标机密钥准备（安装前一次性）

```bash
ssh root@<host>
install -d -m 0750 /etc/sdkwork/payment
# 生成 64 字节主密钥（OpenSSL）；务必纳入密钥托管备份——丢失后已落库凭据将不可解密
openssl rand -base64 64 > /etc/sdkwork/payment/payment-credential-master.secret
chmod 0600 /etc/sdkwork/payment/payment-credential-master.secret
```

## 3. 首次安装

```bash
bin/apps-deploy.sh server install production --host ssh://root@<host>
```

deploy 会：推送 tar 包到 `/opt/deploy/sdkwork-payment/packages` → 解包 → 运行包内 `install/install-ubuntu.sh`（二进制 `--version` 门禁 → 建 `sdkwork` 用户 → `/opt/sdkwork/payment` 布局 → 播种 `/etc/sdkwork/payment/payment.env`（0640）→ systemd 单元）→ `enable --now` → `is-active` + `/healthz` 健康门禁。

> ⚠️ 首发开关：`seedOnBoot=false`、`autoMigrate=false` 是生产默认。首次初始化数据库请临时在 `/etc/sdkwork/payment/payment.env` 中加
> `SDKWORK_DATABASE_AUTO_MIGRATE=true`、`SDKWORK_DATABASE_SEED_ON_BOOT=true`，
> 启动成功并验证 schema/种子后**立即移除这两个变量并重启**。种子含生产模板账户（演示性商户号 + `database:` 加密凭据引用），上线前必须在管理端全部替换为真实商户凭据。

## 4. 上线核验清单

1. `systemctl is-active sdkwork-payment` = active；`journalctl -u sdkwork-payment -f` 无启动错误。
2. `curl -s http://127.0.0.1:18094/healthz` 与 `/readyz` 就绪（绑定地址以 env 为准）。
3. 管理端（Provider 管理）核对三个 bootstrap 生产模板账户：替换真实商户号/AppID/商户序列号，轮换凭据（rotate 后 `last_test_status` 才可信）。
4. 确认 `bin/apps-deploy.sh` 的沙箱触发端点在生产不可用（本仓已按部署环境 fail-closed，`/backend/v3/api/payments/dev/sandbox_trigger` 应 403）。
5. 建一条真实金额为 0.01 的订单走通「下单 → PSP 支付 → 回调 → 订单结算」，并做一笔 0.01 退款。
6. 可选：设置 `OTEL_EXPORTER_OTLP_ENDPOINT` 接入 collector；不设置则仅 stdout 日志。

## 5. 升级 / 状态 / 回滚

```bash
bin/apps-package.sh server production && bin/apps-deploy.sh server upgrade production --host ssh://root@<host>
bin/apps-deploy.sh server status production --host ssh://root@<host>
```

回滚：宿主包通道不追踪版本历史（MODULE_BIN_SPEC §4.5），命令会 fail-fast 并指引——重新打包上一版本再执行 `install`。

## 6. 多副本注意

- 凭据主密钥文件必须共享（或接入 KMS 实现 `install_payment_credential_cipher`），否则副本间无法互解凭据。
- 补偿 worker 每副本各跑一份是安全的（claim 为事务级 `FOR UPDATE SKIP LOCKED` + 状态翻转，结算幂等）；副本数 >2 时建议收紧 `SDKWORK_PAYMENT_COMPENSATION_TENANT_LIMIT`。
- 数据库连接池上限 ≥ 2×并发 checkout（checkout 连接门禁按池容量一半限流）；推荐 ≥ 4。
