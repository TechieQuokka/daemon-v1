# Controller Developer Scenario Guide

Companion to [API-Controller.md](./API-Controller.md). This document covers connection lifecycle, concurrency, edge cases, and behavioral details that a Controller developer needs beyond the message reference.

---

## 1. Connection Lifecycle

### Connecting

1. Open a TCP connection to the daemon (default `127.0.0.1:9000`).
2. There is **no handshake**. The connection is ready for requests immediately after the TCP handshake completes.
3. Encoding is **UTF-8**. Each message is a single line of JSON terminated by `\n`.

### Disconnecting

- Either side may close the TCP connection at any time.
- On disconnect, the daemon logs "Connection closed" and cleans up the per-connection read/write tasks.
- **Subscriptions are NOT automatically cleaned up** on disconnect. The `subscriber_id` handles remain in daemon memory until the daemon restarts. This is a known limitation — long-running controllers that reconnect should track and re-create subscriptions rather than relying on old `subscriber_id` values, because the `bus.recv` receiver channel becomes inaccessible after disconnect.

### Multiple Connections

- The daemon accepts **multiple simultaneous TCP connections**.
- Each connection is handled independently in its own async task.
- All connections share the same `CommandHandler` — actions from any connection affect global daemon state (modules, data, bus).

---

## 2. Request/Response Concurrency

### Strictly Sequential Per Connection

The daemon processes requests **one at a time per connection**:

```
Controller sends request A  →  Daemon processes A  →  Daemon sends response A
Controller sends request B  →  Daemon processes B  →  Daemon sends response B
```

The daemon reads one line, parses it, handles it, writes the response, then reads the next line. **Pipelining is not supported** — if you send request B before receiving response A, request B will be queued in the TCP buffer and processed only after A completes.

### Implication for `bus.recv`

`bus.recv` blocks the connection's processing loop until an event arrives or the timeout expires. During this time, **no other requests on the same connection will be processed**.

Additionally, `bus.recv` holds an internal subscription lock for the entire duration of the wait. Since all connections share the same `CommandHandler`, **a pending `bus.recv` on any connection blocks `bus.subscribe` and `bus.recv` calls on all other connections** until the first call completes or times out. Non-bus actions (`module.start`, `data.set`, etc.) on other connections are **not affected**.

This is an intentional design choice for stability — sequential access to subscription state prevents race conditions in event delivery.

**Recommended pattern**: Use a dedicated TCP connection for `bus.recv` polling, separate from the connection used for command requests. Avoid running multiple concurrent `bus.recv` calls across connections — use a single event-polling connection with short timeouts instead.

```
Connection 1 (commands):  module.start, data.set, bus.publish, ...
Connection 2 (events):    bus.subscribe → bus.recv (loop, short timeout)
```

---

## 3. Request ID (`id` field)

- The `id` is an opaque string — format is entirely up to the controller.
- The daemon echoes it back in the response without modification.
- **Uniqueness is not enforced** — the daemon does not track or deduplicate IDs. If you send two requests with the same ID, you will get two responses with the same ID.
- When the daemon cannot parse a request at all (malformed JSON), the response uses `id: "unknown"`.

---

## 4. Malformed Request Handling

If the daemon receives a line that is not valid JSON or does not match the `ControllerRequest` schema:

```json
← {"id": "unknown", "success": false, "error": "Invalid request: <parse error details>"}
```

The connection remains open — a parse error does not cause disconnection.

If the line exceeds the maximum frame length (tokio `LinesCodec` default: **no explicit limit** in this codebase, but constrained by available memory), the read will fail and the connection is closed.

---

## 5. `bus.subscribe` — Controller vs Module

- **Controllers have no topic restrictions.** Unlike modules, a controller can subscribe to any topic pattern including `*`, `#`, `other_module.events`, etc.
- Each call to `bus.subscribe` creates a new, independent subscription with a unique `subscriber_id` (`controller:{uuid}`).
- Subscribing to the same topic pattern multiple times creates multiple subscriptions — each `bus.recv` call consumes from one specific subscription.

---

## 6. `bus.recv` — Detailed Behavior

### Timeout

- Default: 30000ms (30 seconds).
- When the timeout expires, the response is `{"timeout": true}` — this is a **success** response (`success: true`), not an error.
- Typical pattern: loop `bus.recv` calls, handling both events and timeouts.

### Channel Closure

- If the internal subscription channel is closed (e.g., daemon shutting down), `bus.recv` returns an **error**: `"Subscription channel closed"`.
- If the `subscriber_id` does not exist (typo, or stale from a previous session), `bus.recv` returns an **error**: `"Subscriber not found"`.

### Event Fields

The `data` field in the `bus.recv` result corresponds to the `payload` field of the internal `BusMessage`. The `timestamp` is Unix seconds (not milliseconds).

---

## 7. `data.set` — Priority When Both `value` and `path` Are Given

If both `value` and `path` are present in params, **`path` takes priority** — the value is stored as a file reference and the `value` field is silently ignored.

If neither is present, the response is an error: `"Missing 'value' or 'path' field"`.

---

## 8. `data.get` — File Reference Detection

The data layer distinguishes inline values from file references by checking whether the stored string **starts with the configured `data_layer_path`** (default: `"/data_layer"`). This means:

- If you `data.set` with `value: "/data_layer/foo.txt"` (a string that happens to start with the data layer path), a subsequent `data.get` will return it as `path`, not `value`.
- If you need to store a string that looks like a file path, be aware of this behavior.

---

## 9. Data Layer Eviction

The data layer uses a **SIEVE cache** with a configurable maximum key count (default: 10,000). When the cache is full:

- New `data.set` calls trigger eviction of the least-recently-useful key.
- Evicted keys silently disappear — `data.get` will return `null`.
- There is no notification of eviction.

---

## 10. `module.start` — Config Forwarding

- The `config` param defaults to JSON `null` (not `{}`) if omitted, because the handler does `params["config"].clone()` on a missing key.
- The `name` becomes both the module ID and the `module_name` sent in the `init` message to the module. It must be unique among currently **active** modules (those in the internal process table).

---

## 11. `module.command` — Payload Stripping

The handler removes `module` and `id` keys from the params object before forwarding to the module. The remaining fields become the `command` payload via `#[serde(flatten)]`.

**Caveat**: If your payload contains a key named `"cmd"`, it will conflict with the serde tag field in the serialized `DaemonToModule::Command` message. Avoid using `"cmd"` as a payload key.

---

## 12. `module.stop` — Shutdown Sequence

1. Daemon sends `{"cmd": "shutdown", "force": false, "timeout": <ms>}` to the module's stdin.
2. Daemon waits up to `timeout` ms for the process to exit.
3. If the process does not exit in time, the daemon **force-kills** it (SIGKILL / TerminateProcess).
4. Module status transitions: `running` → `stopping` → `stopped`.

---

## 13. `daemon.shutdown` — Behavior

- Returns `{"status": "shutting_down"}` immediately.
- The actual shutdown is asynchronous — all modules are stopped, then the TCP server closes.
- After sending this request, expect the connection to close shortly.
- If the shutdown channel is not configured, returns an error: `"Shutdown not available"`.

---

## 14. Module Restart

After `module.stop` completes, the module's process entry is removed from the active process table. You **can** call `module.start` again with the same `name` to restart the module. The previous info entry (status, subscriptions, etc.) is overwritten with the new instance.

```
→ module.stop {id: "calc"}     ← status: "stopped"
→ module.start {name: "calc"}  ← module_id: "calc"   (new process, fresh state)
```

Note: the old module's bus subscriptions remain as stale entries (see section 16). The new module starts with no subscriptions.

---

## 15. Subscription Cleanup on Module Stop

When a module is stopped, its bus subscriptions are **NOT automatically removed**. The subscription entries remain in the bus registry with dead sender channels. This means:

- `daemon.status` may report inflated `subscribers` counts.
- This is a known limitation cleaned up only on daemon restart.
- Controller developers should be aware of this when monitoring subscriber counts.

---

## 16. Bus Delivery Guarantees

| Property | Guarantee |
|----------|-----------|
| **Ordering** | **FIFO** — all messages are processed through a single sequential channel. Subscribers receive events in publish order. |
| **Delivery** | **At-most-once** — failed deliveries are logged and skipped. No retry mechanism. |
| **Backpressure** | **None** — unbounded channels. A slow `bus.recv` consumer does not block publishers but may accumulate memory. |
| **Durability** | **None** — all messages are in-memory only. Daemon restart loses all pending messages. |

**Implication for `bus.recv`**: If the controller is slow to call `bus.recv`, events accumulate in the subscription's unbounded channel. There is no overflow protection — poll frequently or use short timeouts.

---

## 17. Large Payload Strategy

The bus is designed for **lightweight event notifications**, not bulk data transfer. Sending large payloads through the bus increases memory pressure on unbounded channels and delays event delivery to all subscribers.

**Recommended pattern — Data Layer + Bus notification**:

1. Store the large result in the data layer (`data.set` with a known key or file reference).
2. Publish a small notification to the bus containing only the key.
3. Interested subscribers receive the notification and fetch the data via `data.get`.

```
# Step 1: Store large result
→ {"action":"data.set","params":{"key":"analysis.result.42","value":{"rows":100000,"columns":["a","b","c"],"data":"...MB of data..."}},"id":"d1"}
← {"id":"d1","success":true,"result":{"key":"analysis.result.42","status":"set"}}

# Step 2: Notify via bus (lightweight — key only)
→ {"action":"bus.publish","params":{"topic":"analysis.complete","data":{"key":"analysis.result.42"}},"id":"d2"}
← {"id":"d2","success":true,"result":{"status":"published"}}

# Step 3: Subscriber receives notification, fetches data
← {"id":"e5","success":true,"result":{"topic":"analysis.complete","data":{"key":"analysis.result.42"},"timestamp":1710000000}}
→ {"action":"data.get","params":{"key":"analysis.result.42"},"id":"d3"}
← {"id":"d3","success":true,"result":{"key":"analysis.result.42","value":{...}}}
```

For very large data (binary files, datasets), use `data.set` with `path` (file reference) instead of `value` to avoid loading the entire payload into daemon memory.

| Data size | Recommended channel | Example |
|-----------|-------------------|---------|
| Small (< 1 KB) | Bus `data` field directly | Status changes, counters, simple results |
| Medium (1 KB – 1 MB) | Data Layer (inline `value`) + Bus notification | JSON results, config snapshots |
| Large (> 1 MB) | Data Layer (file `path`) + Bus notification | Datasets, binary files, logs |

### Data Lifecycle — Cleanup Responsibility

The data layer uses a SIEVE cache with a finite key capacity (default 10,000). SIEVE eviction is a **safety net**, not a cleanup strategy. Developers are responsible for deleting data they no longer need.

- **Producer cleanup**: After all consumers have read the data, the producer deletes it.
- **Consumer cleanup**: The consumer deletes the key after fetching it, if it is the sole consumer.

If data is not explicitly deleted, it occupies cache capacity until SIEVE evicts it — potentially displacing other active data without warning.

```
# After consumer has fetched the result:
→ {"action":"data.delete","params":{"key":"analysis.result.42"},"id":"d4"}
← {"id":"d4","success":true,"result":{"key":"analysis.result.42","deleted":true}}
```

---

## 18. Error Response Catalog

| Condition | Error string |
|-----------|-------------|
| Unknown action | `"Unknown action: {name}"` |
| Missing params object | `"Missing parameters"` |
| Missing required field | `"Missing '{field}' field"` |
| Missing value or path | `"Missing 'value' or 'path' field"` |
| Module not found | `"Module '{id}' not found"` |
| Module already running | `"Module '{id}' already running"` |
| Subscriber not found | `"Subscriber not found"` |
| Channel closed | `"Subscription channel closed"` |
| Shutdown unavailable | `"Shutdown not available"` |
| Malformed JSON | `"Invalid request: {serde error}"` |

---

## 19. Typical Session Example

```
# Connection 1 — Command channel

→ {"action":"module.start","params":{"name":"calc","path":"./modules/calc","config":{"precision":10}},"id":"1"}
← {"id":"1","success":true,"result":{"module_id":"calc"}}

→ {"action":"module.list","id":"2"}
← {"id":"2","success":true,"result":{"modules":[{"id":"calc","path":"./modules/calc","status":"starting","pid":12345,"subscriptions":[]}]}}

  ... (wait for module to send init_complete) ...

→ {"action":"health_check","params":{"module":"calc"},"id":"3"}
← {"id":"3","success":true,"result":{"module_id":"calc","status":"running","pid":12345}}

→ {"action":"module.command","params":{"module":"calc","id":"cmd-1","action":"fibonacci","n":30},"id":"4"}
← {"id":"4","success":true,"result":{"status":"sent"}}

→ {"action":"data.get","params":{"key":"calc.last_result"},"id":"5"}
← {"id":"5","success":true,"result":{"key":"calc.last_result","value":832040}}

→ {"action":"module.stop","params":{"id":"calc"},"id":"6"}
← {"id":"6","success":true,"result":{"status":"stopped"}}

→ {"action":"daemon.shutdown","id":"7"}
← {"id":"7","success":true,"result":{"status":"shutting_down"}}
  (connection closes)
```

```
# Connection 2 — Event channel (parallel)

→ {"action":"bus.subscribe","params":{"topic":"calc.*"},"id":"e1"}
← {"id":"e1","success":true,"result":{"subscriber_id":"controller:a1b2c3d4-..."}}

→ {"action":"bus.recv","params":{"subscriber_id":"controller:a1b2c3d4-...","timeout":10000},"id":"e2"}
  ... (blocks until event or timeout) ...
← {"id":"e2","success":true,"result":{"topic":"calc.result","data":{"n":30,"result":832040},"timestamp":1710000000}}

→ {"action":"bus.recv","params":{"subscriber_id":"controller:a1b2c3d4-...","timeout":10000},"id":"e3"}
← {"id":"e3","success":true,"result":{"timeout":true}}
```
