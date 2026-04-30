# Haruki-HMES

本项目用于处理 HarukiBot 的实时订阅消息。当前阶段只服务一个场景：烤森生日材料更新监听。

HMES 被定位为外挂式轻量推送网关，而不是订阅业务的事实来源。HMES 停机时不能影响 Cloud、Toolbox、普通 Bot 指令、用户上传和绘图服务；最多导致订阅消息无法实时送达。订阅和事件数据应由 Cloud 持久化，HMES 只负责 Client 长轮询认证、在线等待队列和事件转发。

## 名词解释

- 用户：`platform + platform_user_id`，当前 Client 侧实际为 QQ。
- Client：`Haruki-Client` 项目，接收 OneBot V11 群消息并调用 Cloud。
- OneBot 账号：OneBot 事件中的 `self_id`。同一个 Client 进程可能中途更换 `self_id`。
- PJSK 账号：`region + uid`。
- Cloud：`Haruki-Cloud` 项目，服务中心，负责指令解析、订阅持久化、权限校验和绘图 API。
- Toolbox：这里专指 `Haruki-Toolbox-Backend`，负责用户验证账号、上传 suite/mysekai/mysekai_birthday_party 数据。
- HMES：`Haruki-HMES`，实时推送网关。
- Drawing：`Haruki-Drawing-API`，使用 PIL 画图。当前能力已足够，不需要为了本功能修改。

## 功能范围

本订阅只用于 `mysekai_birthday_party` 上传后的稀有生日材料监听。

目标材料：

| 参数名 | Masterdata 名称 | 资源 key |
| --- | --- | --- |
| 钻石 | ダイヤモンド | `mysekai_material_12` |
| 夕桐 | 夕桐 | `mysekai_material_5` |
| 四叶草 | 四葉のクローバー | `mysekai_material_20` |

默认只监听钻石。

## 总体原则

1. Cloud 是订阅状态和事件数据的唯一事实来源。
2. HMES 不保存长期订阅，不拥有订阅数据库表。
3. Toolbox 不保存订阅规则，但可以向 Cloud 查询是否需要处理当前上传。
4. Toolbox、Cloud、Client 对 HMES 的调用都必须是可降级的。HMES 不可用时，上传和普通 Bot 能力仍然成功。
5. 订阅只支持群聊，不支持私聊。
6. 每个 `region + uid` 同一时间只允许一个活跃订阅。新订阅覆盖旧订阅，只提示当前用户订阅已更新，不通知旧订阅方。

## 订阅对象

用户只能监听：

1. 自己的默认 PJSK 账号。
2. 通过 `u[i]` 参数指定的、属于自己的已验证 PJSK 账号。

Cloud 必须在创建订阅时校验账号已验证，并解析最终的 `region + uid`。

订阅记录需要绑定：

- `region`
- `uid`
- `platform`
- `platform_user_id`
- `platform_group_id`
- `cloud_bot_id`
- `self_id`
- `materials`
- `expires_at`

其中 `self_id` 必须参与绑定。虽然当前 Client 不同时连接多个 OneBot 账号，但运行中可能更换账号；旧 `self_id` 的订阅不能自动迁移到新账号，避免把推送发到错误 Bot 账号。

## 命令设计

监听命令不进入 Cloud manifest，必须由新版 Client 硬编码识别并调用专用 Cloud API。

监听命令：

- `/烤森生日监听`
- `/mysekai birthday monitor`
- `/ms生日监听`

取消命令：

- `/烤森生日取消监听`
- `/mysekai birthday unmonitor`
- `/ms生日取消监听`

参数：

- 时长：整数，单位分钟。
- 默认时长：90 分钟。
- 最大时长：120 分钟。
- 材料：`钻石`、`夕桐`、`四叶草`，可多选。
- 材料后可跟 `开启` 或 `关闭` 显式指定。
- 无材料参数时默认开启钻石。
- 如果最终所有材料都关闭，Cloud 返回错误，不创建订阅。

示例：

```text
/烤森生日监听
/烤森生日监听 120 钻石 夕桐
/烤森生日监听 60 钻石关闭 四叶草开启
/ms生日监听 u2 90 四叶草
```

## 基本流程

### 1. 创建或更新订阅

1. 用户在群里发送监听命令。
2. Client 识别该命令为本地特殊命令，不走 manifest。
3. Client 调 Cloud 专用订阅 API，并携带 `self_id`、群、用户、原始参数。
4. Cloud 校验：
   - 请求来自已认证 Bot Client。
   - 消息来自群聊。
   - 目标账号为用户默认账号或 `u[i]` 指定账号。
   - 目标账号已验证。
   - 订阅时长不超过 120 分钟。
   - 至少开启一个材料。
5. Cloud 在自己的订阅表中 upsert 订阅。唯一约束为活跃的 `region + uid`。
6. Cloud 生成 HMES 连接认证信息，放入 `client_actions` 返回给 Client。
7. Client 根据 `client_actions` 建立或刷新到 HMES 的 HTTP 长轮询。
8. Client 向群内回复订阅成功或已更新。

### 2. Toolbox 上传并过滤

1. 用户通过 Toolbox 上传 `mysekai_birthday_party` 数据。
2. Toolbox 正常完成解包、校验和 Mongo 更新。
3. Toolbox 以短超时、best-effort 的方式向 Cloud 查询当前 `region + uid` 是否存在活跃订阅。
4. Cloud 返回 active subscription 及材料 ID。
5. Toolbox 如果没有活跃订阅，直接结束额外流程。
6. Toolbox 如果存在活跃订阅，对 `updatedResources.userMysekaiHarvestMaps` 进行过滤：
   - 保留命中材料所在 map。
   - 保留命中材料 drop。
   - 保留同位置的 harvest fixture，用于地图定位。
   - 删除其它非命中资源 drop。
   - 如果全部过滤为空，也要继续上报空结果事件。
7. Toolbox 将过滤后的 payload 写回 Cloud 的订阅事件 API。
8. Cloud 持久化事件并返回 `event_id`。
9. Toolbox 通知 HMES 推送该 `event_id`。HMES 不可用时只记录日志，不影响上传成功。

### 3. HMES 推送和 Client 绘图

1. HMES 收到事件通知后，根据 `subscription_id` 唤醒对应长轮询；如果当前没有等待中的 Client，则暂存在内存队列。
2. Client 的下一次长轮询取得 `event_id`、`subscription_id`、`empty_result`。
3. Client 收到事件后：
   - 如果 `empty_result = true`，直接向群内 at 用户并发送固定文案：`本次生日材料更新未发现你订阅的材料。`
   - 如果 `empty_result = false`，调用 Cloud 绘图 API。
4. Cloud 根据 `event_id` 读取过滤后的 payload，使用现有 MySekai map renderer 和 Drawing 生成图片。
5. Client 将 Cloud 返回的 OneBot segments 主动发送到订阅群，并 at 订阅用户。

### 4. HMES 停机与恢复

HMES 停机时：

- Cloud 订阅仍然有效。
- Toolbox 上传仍然成功。
- Cloud 仍然可以保存订阅事件。
- 实时长轮询推送不可用。

HMES 恢复后：

- Client 自动恢复长轮询并重新认证。
- HMES 可向 Cloud 校验 token，并拉取该订阅未 ACK 的事件后补推。
- Client 处理完成后向 Cloud ACK，由 Cloud 更新事件投递状态。

## 推荐接口

接口路径可按实际项目风格调整，以下为协议边界建议。

### Client -> Cloud：创建或更新订阅

```http
POST /api/v2/bot/:botId/pjsk/mysekai/birthday-monitor
```

请求字段：

```json
{
  "platform": "qq",
  "platform_user_id": "123",
  "platform_group_id": "456",
  "self_id": "789",
  "message": "/烤森生日监听 90 钻石"
}
```

响应字段：

```json
{
  "status": 200,
  "message": "ok",
  "data": [
    {"type": "text", "data": {"text": "烤森生日材料监听已更新，有效期 90 分钟。"}}
  ],
  "client_actions": [
    {
      "type": "hmes_connect",
      "subscription_id": "...",
      "endpoint": "https://hmes.example.com/poll",
      "token": "...",
      "expires_at": 1770000000
    }
  ]
}
```

`client_actions` 只用于新版 Client，本命令不通过 manifest 下发。

### Toolbox -> Cloud：查询是否需要过滤

```http
GET /internal/subscriptions/mysekai-birthday/active?region=jp&uid=123456
```

响应：

```json
{
  "active": true,
  "subscription_id": "...",
  "materials": ["diamond", "yuugiri", "clover"],
  "material_ids": [12, 5, 20],
  "notify_empty": true
}
```

### Toolbox -> Cloud：写入过滤事件

```http
POST /internal/subscription-events/mysekai-birthday
```

请求：

```json
{
  "subscription_id": "...",
  "region": "jp",
  "uid": "123456",
  "upload_time": 1770000000,
  "matched_material_ids": [12],
  "empty_result": false,
  "filtered_payload": {
    "updatedResources": {
      "userMysekaiHarvestMaps": []
    }
  }
}
```

响应：

```json
{
  "event_id": "..."
}
```

### Toolbox -> HMES：触发实时推送

```http
POST /internal/events
```

请求：

```json
{
  "subscription_id": "...",
  "event_id": "...",
  "empty_result": false
}
```

### Client -> HMES：长轮询

```http
GET /poll?subscription_id=...&token=...
```

响应：

```json
{
  "events": [
    {
      "subscription_id": "...",
      "event_id": "...",
      "empty_result": false
    }
  ]
}
```

HMES 每次 poll 都会向 Cloud 校验 token，并优先返回 Cloud 中未 ACK 的事件，因此 HMES 重启不会丢失已写入 Cloud 的订阅事件。

### Client -> Cloud：事件绘图

```http
POST /api/v2/bot/:botId/pjsk/mysekai/birthday-monitor/render
```

请求：

```json
{
  "platform": "qq",
  "platform_user_id": "123",
  "platform_group_id": "456",
  "self_id": "789",
  "subscription_id": "...",
  "token": "...",
  "event_id": "..."
}
```

响应为普通 OneBot message segments。

### Client -> Cloud：事件 ACK

```http
POST /api/v2/bot/:botId/pjsk/mysekai/birthday-monitor/ack
```

请求字段与事件绘图接口一致。Cloud 会校验 `event_id` 属于当前 Bot、群、用户、`self_id` 和订阅 token 后再标记 ACK。

## 数据持久化建议

Cloud 新增订阅相关表。

### `mysekai_birthday_subscriptions`

- `id`
- `region`
- `uid`
- `platform`
- `platform_user_id`
- `platform_group_id`
- `cloud_bot_id`
- `self_id`
- `materials`
- `token`
- `active`
- `expires_at`
- `created_at`
- `updated_at`
- `cancelled_at`

约束：

- 当前实现对 `region + uid` 保持单行唯一，新订阅会覆盖旧订阅内容并清理旧的未 ACK 事件。
- 查询活跃订阅时必须过滤 `expires_at > now()` 和 `active = true`。

### `mysekai_birthday_subscription_events`

- `id`
- `subscription_id`
- `region`
- `uid`
- `platform`
- `platform_user_id`
- `platform_group_id`
- `cloud_bot_id`
- `self_id`
- `upload_time`
- `matched_material_ids`
- `empty_result`
- `filtered_payload`
- `created_at`
- `acknowledged_at`

约束：

- 当前实现不做上传事件去重；如后续 Toolbox 出现重复通知，再增加 `subscription_id + upload_time + source_hash` 去重。

## 实现注意事项

1. Toolbox 到 Cloud/HMES 的额外调用必须短超时、失败不影响上传。
2. Cloud 绘图 API 只接受已认证 Client 调用，且要校验 `event_id` 属于当前 Bot/群/用户/self_id 对应订阅。
3. Client 更换 `self_id` 后，旧订阅不自动迁移。
4. Client 主动发群消息时必须使用订阅 action 捕获的 `platform_group_id` 和 `self_id`；运行中更换 OneBot 账号时旧订阅不自动迁移。
5. 空结果也必须通知用户，固定文案为：`本次生日材料更新未发现你订阅的材料。`
6. Drawing 端无需修改。

## 当前 HMES 环境变量

- `HMES_ADDR`：监听地址，默认由 `HMES_HOST` 和 `HMES_PORT` 组成。
- `HMES_HOST`：默认 `0.0.0.0`。
- `HMES_PORT`：默认 `7910`。
- `HMES_INTERNAL_TOKEN`：Toolbox 调 `/internal/events` 的可选 token。
- `HMES_CLOUD_INTERNAL_BASE_URL`：Cloud 内网地址，必填。
- `HMES_CLOUD_INTERNAL_TOKEN`：Cloud internal API token。
- `HMES_USER_AGENT`：默认 `Haruki-HMES`。
- `HMES_POLL_TIMEOUT_SECONDS`：单次长轮询最长等待时间，默认 25 秒。
- `HMES_CLOUD_TIMEOUT_SECONDS`：HMES 调 Cloud 的超时，默认 5 秒。
