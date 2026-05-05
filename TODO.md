# Haruki-HMES TODO

## 阶段 1：协议和数据模型

- [x] 在 Cloud 中新增烤森生日订阅表 `mysekai_birthday_subscriptions`。
- [x] 在 Cloud 中新增烤森生日事件表 `mysekai_birthday_subscription_events`。
- [x] 为活跃订阅建立 `region + uid` 唯一约束，保证新订阅覆盖旧订阅。
- [x] 定义材料枚举：
  - `diamond` -> `mysekai_material_12`
  - `yuugiri` -> `mysekai_material_5`
  - `clover` -> `mysekai_material_20`
- [x] 定义 HMES 连接 token 格式。当前为 Cloud 随机签发并持久化，HMES 每次向 Cloud 校验。
- [x] 定义 Cloud 返回给 Client 的 `client_actions` 响应字段。

## 阶段 2：Cloud

- [x] 新增非 manifest 命令接口：`/api/v2/bot/:botId/pjsk/mysekai/birthday-monitor`。
- [x] 新增取消订阅接口或命令路径。
- [x] 监听命令只允许群聊。
- [x] 解析命令别名：
  - `/烤森生日监听`
  - `/mysekai birthday monitor`
  - `/ms生日监听`
- [x] 解析取消命令别名：
  - `/烤森生日取消监听`
  - `/mysekai birthday unmonitor`
  - `/ms生日取消监听`
- [x] 支持默认账号和 `u[i]` 指定账号。
- [x] 校验目标账号已验证。
- [x] 支持时长参数，默认 90 分钟，最大 120 分钟。
- [x] 支持材料参数和 `开启` / `关闭` 后缀。
- [x] 无材料参数时默认监听钻石。
- [x] 如果最终所有材料都关闭，返回错误。
- [x] 创建或更新订阅后返回可见消息和 `client_actions`。
- [x] 新增 Cloud -> Toolbox 监听镜像同步接口。
- [x] 新增 Toolbox Redis 暂存过滤事件与读取/ACK 内部接口。
- [x] 新增 Client 请求绘图接口：根据 `event_id` 读取过滤 payload 并调用现有 Drawing 链路。
- [x] 绘图接口校验 `event_id` 属于当前 Bot、群、用户和 `self_id`。
- [x] 支持事件 ACK 或补推状态更新。

## 阶段 3：Toolbox

- [x] 在 `mysekai_birthday_party` 上传成功后，从 Redis 查询 Cloud 下发的监听镜像。
- [x] 没有活跃监听镜像时跳过过滤和推送。
- [x] 有活跃监听镜像时，根据镜像材料 ID 过滤 `updatedResources.userMysekaiHarvestMaps`。
- [x] 过滤规则：
  - 保留命中材料所在 map。
  - 保留命中材料 drop。
  - 保留同位置的 harvest fixture。
  - 删除其它非命中资源 drop。
  - 全部过滤为空且未开启 `notify_empty` 时不创建事件。
- [x] 将过滤后的事件暂存到 Toolbox Redis。
- [x] Toolbox 暂存成功后通知 HMES 推送 `event_id`。
- [x] HMES 通知失败时仅记录日志，不影响上传成功。

## 阶段 4：HMES

- [x] 实现 SSE 推送服务。
- [x] 实现 Client SSE token + subscription_version 认证。
- [x] 维护 `subscription_id + subscription_version -> latest pending event` 的在线映射。
- [x] 接收 Toolbox 或 Cloud 的事件通知。
- [x] 向在线 Client 返回 `subscription_id`、`event_id`、`empty_result`。
- [x] Client 断线后清理等待队列。
- [x] 支持 Cloud 主动关闭指定 `subscription_id + subscription_version` 的 SSE 连接。
- [x] HMES 重启后允许 Client 自动恢复 SSE。
- [x] Client 重新连接时，补推断线期间 HMES 收到的最新事件。
- [x] HMES 不持久化订阅数据，不依赖本地业务数据库启动。

## 阶段 5：Client

- [x] 硬编码识别监听命令，不依赖 manifest。
- [x] 硬编码识别取消监听命令，不依赖 manifest。
- [x] 调 Cloud 订阅接口时携带 `self_id`。
- [x] 处理 Cloud 返回的 `client_actions`。
- [x] 根据 `hmes_sse` action 建立或刷新 SSE 连接。
- [x] 支持 HMES 自动重连。
- [x] 收到 `empty_result = true` 时，主动向订阅群 at 用户并发送：`本次生日材料更新未发现你订阅的材料。`
- [x] 收到 `empty_result = false` 时，调用 Cloud 绘图接口并把返回的 OneBot segments 主动发到订阅群。
- [x] 处理运行中 OneBot `self_id` 更换：旧 `self_id` 订阅不自动迁移。

## 阶段 6：测试和降级验证

- [x] Cloud 命令解析单元测试。
- [ ] Cloud 订阅覆盖、过期、取消测试。
- [ ] Cloud 权限校验测试：默认账号、`u[i]`、非本人账号、未验证账号。
- [x] Toolbox 过滤规则单元测试。
- [ ] Toolbox 在 HMES 不可用时上传不失败的测试。
- [x] HMES SSE 鉴权和推送测试。
- [x] Client 特殊命令和 `client_actions` 测试。
- [x] Toolbox 实机验证：上传、过滤和回调正常。
- [x] HMES 通知桥实机验证：鉴权和通知正常。
- [x] Cloud 绘图实机验证：读取过滤地图并调用画图逻辑正常。
- [x] Client 实机验证：注册、取消注册、收到通知后获取地图正常。
- [x] 端到端联调验证：订阅 -> 上传 -> 过滤 -> 推送 -> 绘图 -> Client 获取地图。

## 后续优化

- [ ] 评估并实施 HMES Rust 重写，保持现有 HTTP/SSE 接口和环境变量兼容。
