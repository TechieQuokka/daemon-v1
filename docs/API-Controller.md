# Controller ↔ Daemon API Reference

## Overview

The Controller communicates with the Daemon over a **TCP** connection using **newline-delimited JSON** (JSON Lines). Each message is a single JSON object terminated by `\n`.

- **Default address**: `127.0.0.1:9000` (configurable)
- **Protocol**: Request/Response — the Controller sends a request, the Daemon replies with exactly one response sharing the same `id`.

## Request / Response Envelope

### Request

```json
{"action": "module.start", "params": {"name": "calculator", "path": "./modules/calc"}, "id": "req-1"}
```

| Field    | Type   | Required | Description                              |
|----------|--------|----------|------------------------------------------|
| `action` | string | yes      | Action name (see sections below)         |
| `params` | object | no       | Action-specific parameters               |
| `id`     | string | yes      | Caller-generated request identifier      |

### Response

```json
{"id": "req-1", "success": true, "result": {"module_id": "calculator"}}
```

| Field     | Type   | Required | Description                                      |
|-----------|--------|----------|--------------------------------------------------|
| `id`      | string | yes      | Echoed request `id`                              |
| `success` | bool   | yes      | `true` on success, `false` on error              |
| `result`  | any    | no       | Present when `success` is `true`                 |
| `error`   | string | no       | Present when `success` is `false`                |

---

## Actions — Module Management

### `module.start`

Start a new module process.

**Params**

| Field    | Type   | Required | Description                              |
|----------|--------|----------|------------------------------------------|
| `name`   | string | yes      | Module identifier (must be unique)       |
| `path`   | string | yes      | Filesystem path to the module executable |
| `config` | object | no       | Configuration object passed to the module on init |

**Result**

| Field       | Type   | Description                  |
|-------------|--------|------------------------------|
| `module_id` | string | The ID of the started module |

**Example**

```json
→ {"action": "module.start", "params": {"name": "calculator", "path": "./modules/calc", "config": {"precision": 10}}, "id": "1"}
← {"id": "1", "success": true, "result": {"module_id": "calculator"}}
```

---

### `module.stop`

Stop a running module. Sends a graceful shutdown; force-kills if the timeout expires.

**Params**

| Field     | Type | Required | Default | Description                        |
|-----------|------|----------|---------|------------------------------------|
| `id`      | string | yes    |         | Module ID to stop                  |
| `timeout` | u64  | no       | 5000    | Graceful shutdown timeout (ms)     |

**Result**

| Field    | Type   | Description         |
|----------|--------|---------------------|
| `status` | string | Always `"stopped"`  |

**Example**

```json
→ {"action": "module.stop", "params": {"id": "calculator", "timeout": 3000}, "id": "2"}
← {"id": "2", "success": true, "result": {"status": "stopped"}}
```

---

### `module.list`

List all registered modules and their current state. Includes modules in **all statuses** — `running`, `starting`, `stopping`, and `stopped`. Use the `status` field to filter if only active modules are needed.

> **Note**: The `modules` count in `daemon.status` reflects only active processes (not `stopped` entries), so it may be lower than the number of entries returned here.

**Params**: none

**Result**

| Field     | Type  | Description         |
|-----------|-------|---------------------|
| `modules` | array | Array of ModuleInfo |

Each element in `modules`:

| Field           | Type     | Description                         |
|-----------------|----------|-------------------------------------|
| `id`            | string   | Module ID                           |
| `path`          | string   | Filesystem path to executable       |
| `status`        | string/object | Module status (see [Module Status Values](#module-status-values)) |
| `pid`           | u32/null | OS process ID, or `null` if not running |
| `subscriptions` | string[] | List of subscribed topic patterns   |

**Example**

```json
→ {"action": "module.list", "id": "3"}
← {"id": "3", "success": true, "result": {"modules": [
    {"id": "calculator", "path": "./modules/calc", "status": "running", "pid": 12345, "subscriptions": ["calculator.*"]}
  ]}}
```

---

### `module.command`

Send a free-form command to a specific module. The daemon strips `module` and `id` from params and forwards the remaining fields as the command payload.

**Params**

| Field    | Type   | Required | Description                                 |
|----------|--------|----------|---------------------------------------------|
| `module` | string | yes      | Target module ID                            |
| `id`     | string | yes      | Command ID (forwarded to the module)        |
| `...`    | any    | no       | All other fields are forwarded as payload   |

**Result**

| Field    | Type   | Description       |
|----------|--------|-------------------|
| `status` | string | Always `"sent"`   |

**Example**

```json
→ {"action": "module.command", "params": {"module": "calculator", "id": "cmd-1", "action": "calculate", "n": 30}, "id": "4"}
← {"id": "4", "success": true, "result": {"status": "sent"}}
```

The module receives on stdin:

```json
{"cmd": "command", "id": "cmd-1", "action": "calculate", "n": 30}
```

---

### `health_check`

Check the health of a specific module.

**Params**

| Field    | Type   | Required | Description     |
|----------|--------|----------|-----------------|
| `module` | string | yes      | Module ID       |

**Result**

| Field       | Type      | Description                    |
|-------------|-----------|--------------------------------|
| `module_id` | string    | Echoed module ID               |
| `status`    | string/object | Current module status       |
| `pid`       | u32/null  | OS process ID, or `null`       |

**Example**

```json
→ {"action": "health_check", "params": {"module": "calculator"}, "id": "5"}
← {"id": "5", "success": true, "result": {"module_id": "calculator", "status": "running", "pid": 12345}}
```

---

## Actions — Data Layer

### `data.set`

Store a value (inline JSON) or a file reference. Exactly one of `value` or `path` must be provided.

**Params**

| Field   | Type   | Required | Description                                  |
|---------|--------|----------|----------------------------------------------|
| `key`   | string | yes      | Storage key                                  |
| `value` | any    | no       | Inline JSON value to store                   |
| `path`  | string | no       | Filesystem path for large/binary data        |

**Result**

| Field    | Type   | Description            |
|----------|--------|------------------------|
| `key`    | string | Echoed key             |
| `status` | string | Always `"set"`         |

**Example**

```json
→ {"action": "data.set", "params": {"key": "config.theme", "value": "dark"}, "id": "6"}
← {"id": "6", "success": true, "result": {"key": "config.theme", "status": "set"}}
```

---

### `data.get`

Retrieve a stored value. Returns `value` for inline data, `path` for file references, or `null` if the key does not exist.

**Params**

| Field | Type   | Required | Description |
|-------|--------|----------|-------------|
| `key` | string | yes      | Storage key |

**Result**

| Field   | Type       | Description                                    |
|---------|------------|------------------------------------------------|
| `key`   | string     | Echoed key                                     |
| `value` | any/null   | Inline value (present for inline entries or when key not found) |
| `path`  | string     | File path (present for file-reference entries) |

**Example (inline)**

```json
→ {"action": "data.get", "params": {"key": "config.theme"}, "id": "7"}
← {"id": "7", "success": true, "result": {"key": "config.theme", "value": "dark"}}
```

**Example (not found)**

```json
← {"id": "7", "success": true, "result": {"key": "missing.key", "value": null}}
```

---

### `data.delete`

Delete a stored key.

**Params**

| Field | Type   | Required | Description |
|-------|--------|----------|-------------|
| `key` | string | yes      | Storage key |

**Result**

| Field     | Type   | Description                         |
|-----------|--------|-------------------------------------|
| `key`     | string | Echoed key                          |
| `deleted` | bool   | `true` if key existed and was removed |

**Example**

```json
→ {"action": "data.delete", "params": {"key": "config.theme"}, "id": "8"}
← {"id": "8", "success": true, "result": {"key": "config.theme", "deleted": true}}
```

---

### `data.list`

List all stored keys.

**Params**: none

**Result**

| Field  | Type     | Description             |
|--------|----------|-------------------------|
| `keys` | string[] | All keys in the store   |

**Example**

```json
→ {"action": "data.list", "id": "9"}
← {"id": "9", "success": true, "result": {"keys": ["config.theme", "session.token"]}}
```

---

## Actions — Bus

### `bus.publish`

Publish a message to the bus. All subscribers whose topic pattern matches will receive the event.

**Params**

| Field   | Type   | Required | Description              |
|---------|--------|----------|--------------------------|
| `topic` | string | yes      | Topic string             |
| `data`  | any    | no       | Event payload (default `{}`) |

**Result**

| Field    | Type   | Description            |
|----------|--------|------------------------|
| `status` | string | Always `"published"`   |

**Example**

```json
→ {"action": "bus.publish", "params": {"topic": "system.alert", "data": {"level": "warning"}}, "id": "10"}
← {"id": "10", "success": true, "result": {"status": "published"}}
```

---

### `bus.subscribe`

Create a controller-side subscription. Returns a `subscriber_id` that must be used with `bus.recv` to consume events.

**Params**

| Field   | Type   | Required | Description              |
|---------|--------|----------|--------------------------|
| `topic` | string | yes      | Topic pattern to match   |

**Result**

| Field           | Type   | Description                       |
|-----------------|--------|-----------------------------------|
| `subscriber_id` | string | Subscription handle (format: `controller:{uuid}`) |

**Example**

```json
→ {"action": "bus.subscribe", "params": {"topic": "system.*"}, "id": "11"}
← {"id": "11", "success": true, "result": {"subscriber_id": "controller:a1b2c3d4-..."}}
```

---

### `bus.recv`

Long-poll for the next event on a subscription. Blocks until an event arrives or the timeout expires.

**Params**

| Field           | Type   | Required | Default | Description                      |
|-----------------|--------|----------|---------|----------------------------------|
| `subscriber_id` | string | yes      |         | Subscription handle from `bus.subscribe` |
| `timeout`       | u64    | no       | 30000   | Max wait time in milliseconds    |

**Result (event received)**

| Field       | Type   | Description                        |
|-------------|--------|------------------------------------|
| `topic`     | string | Topic the event was published on   |
| `data`      | any    | Event payload                      |
| `timestamp` | u64    | Unix timestamp (seconds)           |

**Result (timeout)**

| Field     | Type | Description     |
|-----------|------|-----------------|
| `timeout` | bool | Always `true`   |

**Example (event)**

```json
→ {"action": "bus.recv", "params": {"subscriber_id": "controller:a1b2c3d4-...", "timeout": 5000}, "id": "12"}
← {"id": "12", "success": true, "result": {"topic": "system.alert", "data": {"level": "warning"}, "timestamp": 1710000000}}
```

**Example (timeout)**

```json
← {"id": "12", "success": true, "result": {"timeout": true}}
```

---

## Actions — Daemon

### `daemon.status`

Get daemon-wide status counters.

**Params**: none

**Result**

| Field         | Type   | Description                      |
|---------------|--------|----------------------------------|
| `modules`     | u64    | Number of active module processes (excludes `stopped` entries visible in `module.list`) |
| `subscribers` | u64    | Number of active bus subscribers |
| `data_keys`   | u64    | Number of keys in the data layer |
| `status`      | string | Always `"running"`               |

**Example**

```json
→ {"action": "daemon.status", "id": "13"}
← {"id": "13", "success": true, "result": {"modules": 2, "subscribers": 5, "data_keys": 10, "status": "running"}}
```

---

### `daemon.shutdown`

Request a graceful daemon shutdown. All modules are stopped before the daemon exits.

**Params**: none

**Result**

| Field    | Type   | Description               |
|----------|--------|---------------------------|
| `status` | string | Always `"shutting_down"`  |

**Example**

```json
→ {"action": "daemon.shutdown", "id": "14"}
← {"id": "14", "success": true, "result": {"status": "shutting_down"}}
```

---

## Module Status Values

| Value                      | Description                                   |
|----------------------------|-----------------------------------------------|
| `"starting"`               | Process spawned, waiting for `init_complete`  |
| `"running"`                | Module sent `init_complete`, fully operational |
| `"stopping"`               | Shutdown initiated, waiting for process exit  |
| `"stopped"`                | Process exited cleanly                        |
| `{"crashed":{"reason":"..."}}` | Process exited abnormally                 |

`crashed` is serialized as a JSON object with a `reason` field, not a plain string.

---

## Error Handling

When an action fails, the response has `success: false` and an `error` string describing what went wrong. Common error conditions:

- Unknown action name → `"Unknown action: <name>"`
- Missing required params → `"Missing parameters"` or `"Missing '<field>' field"`
- Module not found → `"Module '<id>' not found"`
- Module already running → `"Module '<id>' already running"`
- Subscriber not found → `"Subscriber not found"`
- Subscription channel closed → `"Subscription channel closed"`

**Example**

```json
← {"id": "99", "success": false, "error": "Module 'unknown' not found"}
```

---

## Topic Pattern Syntax

Topic strings use `.` as a segment separator. Patterns used in `bus.subscribe` support two wildcards:

| Pattern | Matches                         | Example pattern   | Matches                          | Does not match             |
|---------|---------------------------------|-------------------|----------------------------------|----------------------------|
| `*`     | Exactly one segment             | `user.*`          | `user.created`, `user.deleted`   | `user.profile.updated`     |
| `#`     | Zero or more segments           | `user.#`          | `user.created`, `user.profile.updated` | —                    |

Wildcards can appear in any segment position. Examples:

- `*.created` → matches `user.created`, `post.created`
- `system.#` → matches `system.shutdown`, `system.module.started`
- `user.*.#` → matches `user.profile.updated`, `user.session.auth.expired`
