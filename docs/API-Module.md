# Daemon ↔ Module API Reference

## Overview

Modules are **child processes** spawned by the Daemon. Communication uses **stdin** (Daemon → Module) and **stdout** (Module → Daemon) with **newline-delimited JSON** (JSON Lines). Each message is a single JSON object terminated by `\n`.

- **Daemon → Module** messages are discriminated by the `"cmd"` field.
- **Module → Daemon** messages are discriminated by the `"type"` field.
- Modules **must not** write anything to stdout that is not valid JSON Lines (use stderr for debug output).

## Module Lifecycle

```
Spawned → [recv init] → [send init_complete] → Running → [recv shutdown] → Stopped
           (starting)                           (running)    (stopping)      (stopped)
```

1. The Daemon spawns the module process and sends an `init` message on stdin.
2. The module performs initialization and **must** send `init_complete` on stdout. Until this is sent, the module status remains `starting`.
3. The module is now `running` and can send/receive messages normally.
4. When the Daemon needs to stop the module, it sends a `shutdown` message. The module should clean up and exit.
5. If the module does not exit within the timeout, the Daemon force-kills the process.

---

## Daemon → Module Messages

All messages contain a `"cmd"` field that identifies the message type. Field names use `snake_case`.

### `init`

Sent once immediately after the module process is spawned.

| Field         | Type   | Required | Description                              |
|---------------|--------|----------|------------------------------------------|
| `cmd`         | string | yes      | Always `"init"`                          |
| `module_name` | string | yes      | The module's assigned ID                 |
| `config`      | object | yes      | Configuration object from `module.start` |

**Example**

```json
{"cmd": "init", "module_name": "calculator", "config": {"precision": 10}}
```

---

### `command`

A free-form command forwarded from the Controller via `module.command`. The `id` field and all payload fields are **flattened** into the top-level object (via `#[serde(flatten)]`).

| Field | Type   | Required | Description                                    |
|-------|--------|----------|------------------------------------------------|
| `cmd` | string | yes      | Always `"command"`                             |
| `id`  | string | yes      | Command ID (use this in `ack`/`error` replies) |
| `...` | any    | —        | All other payload fields appear at top level   |

**Example**

```json
{"cmd": "command", "id": "cmd-1", "action": "calculate", "n": 30}
```

---

### `event`

A bus event delivered to the module via its active subscriptions.

| Field       | Type   | Required | Description                                    |
|-------------|--------|----------|------------------------------------------------|
| `cmd`       | string | yes      | Always `"event"`                               |
| `topic`     | string | yes      | Topic the event was published on               |
| `data`      | any    | no       | Event payload (omitted if none)                |
| `publisher` | string | yes      | Source identifier (`"controller"`, `"system"`, or module ID) |
| `timestamp` | u64    | yes      | Unix timestamp in seconds                      |

**Example**

```json
{"cmd": "event", "topic": "system.shutdown", "data": {"reason": "maintenance"}, "publisher": "system", "timestamp": 1710000000}
```

---

### `shutdown`

Requests the module to shut down gracefully.

| Field     | Type   | Required | Default | Description                              |
|-----------|--------|----------|---------|------------------------------------------|
| `cmd`     | string | yes      |         | Always `"shutdown"`                      |
| `force`   | bool   | no       | `false` | If `true`, module should exit immediately |
| `timeout` | u64    | no       |         | Suggested time budget in milliseconds    |

**Example**

```json
{"cmd": "shutdown", "force": false, "timeout": 5000}
```

---

### `data_response`

Response to a `data_read` request. Contains inline `value`, a file `path`, or neither (key not found).

| Field   | Type   | Required | Description                            |
|---------|--------|----------|----------------------------------------|
| `cmd`   | string | yes      | Always `"data_response"`               |
| `key`   | string | yes      | The requested key                      |
| `value` | any    | no       | Inline value (omitted if file or absent) |
| `path`  | string | no       | File path (omitted if inline or absent)  |

**Example (inline value)**

```json
{"cmd": "data_response", "key": "config.theme", "value": "dark"}
```

**Example (not found)**

```json
{"cmd": "data_response", "key": "missing.key"}
```

---

## Module → Daemon Messages

All messages contain a `"type"` field that identifies the message type. Field names use `snake_case`.

### `init_complete`

**Must** be sent after the module finishes initialization. Transitions the module status from `starting` to `running`. Has no additional fields.

| Field  | Type   | Required | Description              |
|--------|--------|----------|--------------------------|
| `type` | string | yes      | Always `"init_complete"` |

**Example**

```json
{"type": "init_complete"}
```

---

### `ack`

Acknowledge receipt/completion of a command.

| Field  | Type   | Required | Description                    |
|--------|--------|----------|--------------------------------|
| `type` | string | yes      | Always `"ack"`                 |
| `id`   | string | yes      | The `id` from the command      |

**Example**

```json
{"type": "ack", "id": "cmd-1"}
```

---

### `error`

Report an error for a specific command.

| Field     | Type   | Required | Description                    |
|-----------|--------|----------|--------------------------------|
| `type`    | string | yes      | Always `"error"`               |
| `id`      | string | yes      | The `id` from the command      |
| `code`    | u32    | yes      | Error code (see [Error Code Ranges](#error-code-ranges)) |
| `message` | string | no       | Human-readable description     |

**Example**

```json
{"type": "error", "id": "cmd-1", "code": 1002, "message": "integer overflow"}
```

---

### `publish`

Publish an event to the message bus.

| Field      | Type   | Required | Description                |
|------------|--------|----------|----------------------------|
| `type`     | string | yes      | Always `"publish"`         |
| `topic`    | string | yes      | Topic string               |
| `metadata` | object | yes      | Event payload              |

**Example**

```json
{"type": "publish", "topic": "calculator.result", "metadata": {"input": 30, "output": 832040}}
```

---

### `subscribe_request`

Subscribe to a topic pattern. Subject to [Subscription Rules](#subscription-rules).

| Field   | Type   | Required | Description              |
|---------|--------|----------|--------------------------|
| `type`  | string | yes      | Always `"subscribe_request"` |
| `topic` | string | yes      | Topic pattern to subscribe to |

**Example**

```json
{"type": "subscribe_request", "topic": "calculator.*"}
```

---

### `unsubscribe_request`

Unsubscribe from a topic pattern.

| Field   | Type   | Required | Description              |
|---------|--------|----------|--------------------------|
| `type`  | string | yes      | Always `"unsubscribe_request"` |
| `topic` | string | yes      | Topic pattern to unsubscribe from |

**Example**

```json
{"type": "unsubscribe_request", "topic": "calculator.*"}
```

---

### `data_write`

Write a value to the shared data layer. Provide exactly one of `value` or `path`.

| Field   | Type   | Required | Description                        |
|---------|--------|----------|------------------------------------|
| `type`  | string | yes      | Always `"data_write"`              |
| `key`   | string | yes      | Storage key                        |
| `value` | any    | no       | Inline JSON value                  |
| `path`  | string | no       | File path for large/binary data    |

**Example**

```json
{"type": "data_write", "key": "calculator.last_result", "value": 832040}
```

---

### `data_read`

Request a value from the data layer. The Daemon responds with a `data_response` message on stdin.

| Field  | Type   | Required | Description    |
|--------|--------|----------|----------------|
| `type` | string | yes      | Always `"data_read"` |
| `key`  | string | yes      | Storage key    |

**Example**

```json
{"type": "data_read", "key": "config.theme"}
```

---

### `data_delete`

Delete a key from the data layer.

| Field  | Type   | Required | Description        |
|--------|--------|----------|--------------------|
| `type` | string | yes      | Always `"data_delete"` |
| `key`  | string | yes      | Storage key        |

**Example**

```json
{"type": "data_delete", "key": "calculator.last_result"}
```

---

### `log`

Send a log message to the Daemon's logging system.

| Field     | Type   | Required | Default  | Description                                    |
|-----------|--------|----------|----------|------------------------------------------------|
| `type`    | string | yes      |          | Always `"log"`                                 |
| `message` | string | yes      |          | Log message text                               |
| `level`   | string | no       | `"info"` | One of: `error`, `warn`, `info`, `debug`, `trace` |

**Example**

```json
{"type": "log", "message": "Initialization complete", "level": "info"}
```

---

## Subscription Rules

Modules are restricted in which topics they can subscribe to. This enforces **module isolation** — modules cannot eavesdrop on other modules' events.

**Allowed subscriptions** for a module with ID `{module_id}`:

| Pattern                | Description                  |
|------------------------|------------------------------|
| `system`               | Exact system topic           |
| `system.*`             | System events (one level)    |
| `system.#`             | System events (all levels)   |
| `system.<subtopic>`    | Specific system subtopic     |
| `{module_id}`          | Own exact topic              |
| `{module_id}.*`        | Own events (one level)       |
| `{module_id}.#`        | Own events (all levels)      |
| `{module_id}.<subtopic>` | Own specific subtopic      |

**Rejected subscriptions**:

| Pattern              | Reason                                     |
|----------------------|--------------------------------------------|
| `*`                  | Global single wildcard — too broad         |
| `#`                  | Global multi wildcard — too broad          |
| `other_module.*`     | Cross-module communication not allowed     |
| `other_module.topic` | Cross-module communication not allowed     |

---

## Topic Pattern Syntax

Topic strings use `.` as a segment separator. Two wildcard tokens are supported:

| Token | Matches                | Example             | Matches                                      | Does not match         |
|-------|------------------------|----------------------|-----------------------------------------------|------------------------|
| `*`   | Exactly one segment    | `user.*`             | `user.created`, `user.deleted`                | `user.profile.updated` |
| `#`   | Zero or more segments  | `user.#`             | `user.created`, `user.profile.updated`        | —                      |

Wildcards can appear in any segment position:

- `*.created` → `user.created`, `post.created`
- `system.#` → `system.shutdown`, `system.module.started`
- `user.*.#` → `user.profile.updated`, `user.session.auth.expired`

---

## Error Code Ranges

| Range       | Owner          | Description                         |
|-------------|----------------|-------------------------------------|
| 0000–0999   | Daemon         | Reserved for daemon-internal errors |
| 1000–1999   | Module-defined | First module range (e.g., calculator) |
| 2000–2999   | Module-defined | Second module range (e.g., logger)  |
| 3000+       | Module-defined | Additional module ranges            |

**Daemon error codes**:

| Code | Name              | Description              |
|------|-------------------|--------------------------|
| 1    | UnknownCommand    | Unrecognized command     |
| 2    | InvalidFormat     | Malformed message        |
| 3    | ModuleNotFound    | Target module not found  |

Modules define their own codes within their assigned range. Convention: `{range_start} + offset`.

---

## Self-Publish Filtering

Events published by a module are **not** delivered back to that same module, even if the module has a matching subscription. This prevents feedback loops.

For example, if module `calculator` is subscribed to `calculator.*` and publishes to `calculator.result`, the `calculator` module will **not** receive that event. Other subscribers will receive it normally.
