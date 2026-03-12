# Module Developer Scenario Guide

Companion to [API-Module.md](./API-Module.md). This document covers process lifecycle details, buffering, edge cases, and behavioral details that a Module developer needs beyond the message reference.

---

## 1. Process Spawning

The daemon spawns the module as a child process:

```
Command::new(path)
    .stdin(Stdio::piped())      // Daemon writes here
    .stdout(Stdio::piped())     // Daemon reads here
    .stderr(Stdio::inherit())   // Inherited to parent — goes to daemon's terminal
```

### Key points

- **stdin**: Daemon → Module messages (JSON Lines).
- **stdout**: Module → Daemon messages (JSON Lines). **Must** contain only valid JSON Lines — no debug prints, no banners, no prompts.
- **stderr**: Inherited from the daemon process. Use stderr freely for debug logging, stack traces, or diagnostics — it goes directly to the daemon's terminal/log output. The daemon does not read or parse stderr.
- **Encoding**: UTF-8. The daemon uses tokio `LinesCodec` which reads until `\n`.
- **Buffering**: The module **must** flush stdout after each JSON line. In many languages, stdout is line-buffered when connected to a pipe (not a terminal), but some runtimes (e.g., Python) default to block-buffered for pipes. Ensure line-buffered or unbuffered stdout.

### Process environment

- **Command-line arguments**: None. The daemon spawns the module with no arguments (`Command::new(path)` only). All configuration is delivered via the `init` message's `config` field — not via CLI args or environment variables.
- **Environment variables**: Inherited from the daemon process as-is. No additional variables are set.
- **Working directory**: Inherited from the daemon process. Do not assume a specific cwd — use absolute paths for file operations.

---

## 2. Initialization Sequence (Detailed)

```
Daemon                          Module
  |                               |
  |--- spawn process ------------>|
  |                               | (process starts)
  |--- {"cmd":"init",...} ------->|
  |                               | (module initializes)
  |                               | (module MUST send:)
  |<-- {"type":"init_complete"} --|
  |                               |
  | status: starting → running    |
```

### Timing

1. The `init` message is sent **immediately** after spawn — before the daemon even enters the message handler loop. It is **guaranteed to be the first message** the module receives on stdin.
2. There is **no timeout** for `init_complete`. If the module never sends it, the module stays in `starting` status indefinitely. The daemon does not kill the module for being slow to initialize.
3. The module can take as long as needed to initialize (load files, connect to databases, etc.) before sending `init_complete`.

### What happens before `init_complete`?

- The daemon's message handler loop is already running. Any message the module sends **before** `init_complete` will be processed normally.
- You **can** send `publish`, `subscribe_request`, `data_write`, `log`, etc. before `init_complete`. They will work.
- However, the module status remains `starting` until `init_complete` is received. A `health_check` from the Controller will report `"starting"`.

### Config value

- The `config` field in the `init` message is exactly what was passed in `module.start` params.
- If the controller omitted `config`, this will be JSON `null` (not `{}`).
- Always handle `null` config gracefully.

---

## 3. Message Processing Model

The daemon reads module stdout **sequentially** — one line at a time. Each parsed message is processed to completion before the next is read. This means:

- Messages are handled in **FIFO order**.
- A `data_read` request will block the daemon's processing of subsequent module messages until the `data_response` is sent back. In practice this is fast (in-memory cache lookup), but be aware of the sequential nature.
- The module itself should maintain its own **message loop** — continuously read stdin, process messages, and write responses to stdout.

---

## 4. Message Interleaving on stdin

After initialization, the module's stdin receives messages from **multiple sources concurrently**:

- **Commands** from the Controller (via `module.command`)
- **Events** from the bus (via active subscriptions)
- **Data responses** from `data_read` requests
- **Shutdown** from the daemon

These arrive on a **single stdin stream** and can be interleaved in any order:

```
← {"cmd":"command","id":"cmd-1","action":"calculate","n":30}
← {"cmd":"event","topic":"system.heartbeat","data":{},"publisher":"system","timestamp":1710000000}
← {"cmd":"command","id":"cmd-2","action":"status"}
← {"cmd":"data_response","key":"config.theme","value":"dark"}
← {"cmd":"event","topic":"calc.request","data":{"n":40},"publisher":"controller","timestamp":1710000001}
← {"cmd":"shutdown","force":false,"timeout":5000}
```

**The module must not assume any ordering.** In particular:

- A `data_response` may arrive between two `command` messages.
- Multiple `event` messages may arrive while processing a `command`.
- A `shutdown` can arrive at any time.

The recommended approach is a single message loop that dispatches on the `cmd` field (as shown in the example in section 18).

---

## 5. Responding to Commands

When the module receives a `command` message, it should respond with either `ack` or `error` using the same `id`:

```
Daemon → Module:  {"cmd":"command","id":"cmd-1","action":"calculate","n":30}
Module → Daemon:  {"type":"ack","id":"cmd-1"}
```

or

```
Module → Daemon:  {"type":"error","id":"cmd-1","code":1002,"message":"integer overflow"}
```

### Important details

- **`ack` and `error` are informational only.** The daemon logs them but takes no further action. The controller does NOT receive these responses — the controller only gets `{"status":"sent"}` from `module.command`.
- If you need to return results to the controller, **publish them to the bus**:

```
Module → Daemon:  {"type":"publish","topic":"calc.result","metadata":{"input":30,"output":832040}}
```

The controller can subscribe to `calc.*` to receive results.

---

## 6. Naming: `metadata` vs `data`

The publish message uses `metadata` for the event payload:

```json
{"type": "publish", "topic": "calc.result", "metadata": {"input": 30, "output": 832040}}
```

When this event is delivered back to a subscribed module, it appears as `data`:

```json
{"cmd": "event", "topic": "calc.result", "data": {"input": 30, "output": 832040}, "publisher": "calc", "timestamp": 1710000000}
```

Similarly, when a controller receives it via `bus.recv`, the field is also `data`. The rename happens because the bus stores it as `payload` internally, and the event delivery code maps it to `data`.

---

## 7. Subscription Requests — No Response

When you send `subscribe_request`, `unsubscribe_request`, `data_write`, or `data_delete`:

- **There is no response message.** The daemon processes the request silently.
- If it succeeds, events matching the pattern will start arriving.
- If it fails (e.g., subscription rule violation), the daemon logs a warning and returns an error internally, but **no error message is sent back to the module**.

### Recommended defensive pattern

After subscribing, verify by checking whether events arrive. For data operations, use `data_read` afterward if you need confirmation (it is the only data operation that gets a response — `data_response`).

---

## 8. `data_read` — Request/Response Pair

`data_read` is the only module message that triggers a daemon response:

```
Module → Daemon:  {"type":"data_read","key":"config.theme"}
Daemon → Module:  {"cmd":"data_response","key":"config.theme","value":"dark"}
```

### Key not found

```
Daemon → Module:  {"cmd":"data_response","key":"missing.key"}
```

When the key does not exist, **both `value` and `path` are omitted** (not `null`). Check for the presence of the fields, not their values.

### File reference

```
Daemon → Module:  {"cmd":"data_response","key":"bigfile","path":"/data_layer/file123.dat"}
```

When the stored value is a file reference (string starting with the configured `data_layer_path`), only `path` is present.

---

## 9. `command` Payload — Flatten Behavior

The `command` message uses `#[serde(flatten)]` on the payload. This means payload fields are merged into the top-level JSON object:

```json
{"cmd": "command", "id": "cmd-1", "action": "calculate", "n": 30}
```

NOT nested:

```json
{"cmd": "command", "id": "cmd-1", "payload": {"action": "calculate", "n": 30}}
```

### Reserved keys

Avoid these keys in command payloads — they will collide with the envelope:

| Key    | Reason |
|--------|--------|
| `cmd`  | Serde tag discriminator for DaemonToModule |
| `id`   | Command ID field (overwritten/conflicted) |

---

## 10. Shutdown Sequence (Detailed)

```
Daemon                          Module
  |                               |
  |--- {"cmd":"shutdown",         |
  |     "force":false,            |
  |     "timeout":5000} -------->|
  |                               | (module cleans up)
  |                               | (module exits process)
  |<-- process exit --------------|
  |                               |
  | status: stopping → stopped    |
```

### Timeout behavior

1. The daemon sends `shutdown` with `force: false` and the configured timeout.
2. The daemon then waits up to `timeout` ms for the process to exit (using `child.wait()`).
3. If the process exits within the timeout → status becomes `stopped`.
4. If the process does NOT exit within the timeout → the daemon **force-kills** it (OS kill signal). Status still becomes `stopped`.

### What the module should do

1. Receive the `shutdown` message.
2. Flush any pending work, close file handles, etc.
3. **Exit the process** (exit code 0 for clean exit).
4. Do NOT send any special message — just exit.

### `force: true`

When `force` is true, the module should exit **immediately** without cleanup. In the current implementation, the daemon always sends `force: false` for graceful shutdown and only force-kills at the OS level if the timeout expires.

---

## 11. Process Exit / Crash

### Clean exit

When the module process exits (for any reason), the daemon detects the closed stdout pipe:

1. The message handler loop receives `None` from `process.recv()`.
2. The handler logs "Module '{id}' process ended" and stops.
3. **The module status is NOT automatically updated on exit.** If the module exits without being stopped via `module.stop`, it may remain in `running` or `starting` status in the registry until someone calls `health_check` or `module.list`.

### Crash detection

The daemon calls `mark_crashed(id, reason)` only in specific code paths. If the module simply exits unexpectedly, the process handle becomes invalid but the registry entry may not reflect "crashed" status.

**Recommendation**: Modules should send a `log` message with level `error` before exiting abnormally, so the daemon has a record.

---

## 12. Self-Publish Filtering — Details

When a module publishes to the bus, the daemon tags the message with `MessageSource::Module { id: "module_id" }`. When delivering events to subscribers, the event forwarding task checks:

```rust
if let MessageSource::Module { id } = &bus_msg.source {
    if id == &module_id_clone {
        continue;  // skip
    }
}
```

This means:

- Module `calc` publishes to `calc.result` → `calc` does NOT receive it (even if subscribed to `calc.*`).
- Module `logger` subscribed to `calc.*` → `logger` DOES receive it.
- Controller subscribed to `calc.*` → Controller DOES receive it via `bus.recv`.
- Events from `MessageSource::Controller` and `MessageSource::System` are always delivered to all matching subscribers, including the module that might share the same topic namespace.

---

## 13. Subscription Rules — Detailed Examples

Given module ID `fibonacci`:

```
✅ fibonacci.*           → own namespace, one level
✅ fibonacci.#           → own namespace, all levels
✅ fibonacci.result      → own specific subtopic
✅ fibonacci             → own exact topic
✅ system.*              → system events, one level
✅ system.#              → system events, all levels
✅ system.shutdown       → specific system event
✅ system                → exact system topic

❌ *                     → global wildcard rejected
❌ #                     → global wildcard rejected
❌ calculator.*          → cross-module rejected
❌ calculator.result     → cross-module rejected
❌ logger.log            → cross-module rejected
❌ fibonacci_v2.command  → different module (not a prefix match — must be exact ID + dot)
❌ (empty string)        → rejected
```

The validation checks `topic.starts_with("{module_id}.")` or `topic == "{module_id}"`, so a module named `fib` **cannot** subscribe to `fibonacci.command` — the prefix must be an exact match with a dot separator.

---

## 14. `system.*` Events — Currently None Published

The subscription rules allow modules to subscribe to `system.*` and `system.#`, and `MessageSource::System` exists internally. However, **the daemon does not currently publish any system events.** There is no `system.shutdown`, `system.module.started`, or any other system topic being emitted.

- Shutdown notification comes via **stdin** (`{"cmd":"shutdown",...}`), not the bus.
- Module lifecycle events (start/stop/crash of other modules) are not broadcast.

Subscribing to `system.*` is valid and will not error, but no events will arrive until a future version adds system event publishing. Do not rely on bus events for shutdown handling — always handle the `shutdown` command in your stdin message loop.

---

## 15. Subscription Cleanup on Module Stop

When a module is stopped (`module.stop`), the daemon:

1. Aborts the message handler task.
2. Shuts down the module process.

However, **the module's bus subscriptions are NOT automatically removed.** The subscription entries remain in the bus registry. Since the sender channel is dropped when the forwarding task is aborted, the bus will encounter send errors when trying to deliver to these stale subscriptions — these errors are logged as warnings but are otherwise harmless.

**Implications**:

- `daemon.status` may report a higher `subscribers` count than expected after modules are stopped.
- This is a known resource leak that is cleaned up only when the daemon restarts.
- Module developers do not need to take any action — this is a daemon-side limitation.

---

## 16. Bus Delivery Guarantees

The message bus provides specific guarantees that module developers should understand:

| Property | Guarantee |
|----------|-----------|
| **Ordering** | **FIFO** — messages are processed sequentially through a single channel. All subscribers receive events in publish order. |
| **Delivery** | **At-most-once** — if delivery to a subscriber fails (e.g., channel full or closed), the message is dropped for that subscriber. No retry. |
| **Backpressure** | **None** — the bus uses unbounded channels. A slow consumer does not block publishers, but may accumulate unbounded memory. |
| **Durability** | **None** — messages exist only in memory. If the daemon restarts, all pending messages are lost. |

---

## 17. Large Payload Strategy

The bus is designed for **lightweight event notifications**, not bulk data transfer. Sending large payloads through the bus increases memory pressure on unbounded channels and delays event delivery to all subscribers.

**Recommended pattern — Data Layer + Bus notification**:

1. Write the large result to the data layer (`data_write`).
2. Publish a small notification to the bus containing only the key (`publish`).
3. Subscribers receive the notification and fetch the data via `data_read`.

```python
# ❌ Bad — large payload directly in bus
send({
    "type": "publish",
    "topic": "analysis.complete",
    "metadata": {"rows": 100000, "data": "...MB of data..."}  # Avoid!
})

# ✅ Good — store in data layer, notify via bus with key only
send({
    "type": "data_write",
    "key": "analysis.result.42",
    "value": {"rows": 100000, "data": "...MB of data..."}
})
send({
    "type": "publish",
    "topic": "analysis.complete",
    "metadata": {"key": "analysis.result.42"}  # Lightweight notification
})
```

The receiving module fetches the actual data when needed:

```python
elif cmd == "event":
    if msg["topic"] == "analysis.complete":
        key = msg["data"]["key"]
        send({"type": "data_read", "key": key})
        # data_response will arrive on stdin with the full payload
```

For very large data (binary files, datasets), use `data_write` with `path` (file reference) instead of `value` to avoid loading the entire payload into daemon memory.

| Data size | Recommended channel | Example |
|-----------|-------------------|---------|
| Small (< 1 KB) | Bus `metadata` field directly | Status changes, counters, simple results |
| Medium (1 KB – 1 MB) | Data Layer (inline `value`) + Bus notification | JSON results, config snapshots |
| Large (> 1 MB) | Data Layer (file `path`) + Bus notification | Datasets, binary files, logs |

### Data Lifecycle — Cleanup Responsibility

The data layer uses a SIEVE cache with a finite key capacity (default 10,000). SIEVE eviction is a **safety net**, not a cleanup strategy. Developers are responsible for deleting data they no longer need.

- **Producer cleanup**: After all consumers have read the data, the producer deletes it.
- **Consumer cleanup**: The consumer deletes the key after fetching it, if it is the sole consumer.

If data is not explicitly deleted, it occupies cache capacity until SIEVE evicts it — potentially displacing other active data without warning.

```python
# After consumer has fetched the result:
send({"type": "data_delete", "key": "analysis.result.42"})
```

---

## 18. Error Codes — Convention

The module's error codes should fall within its assigned range:

```json
{"type": "error", "id": "cmd-1", "code": 1001, "message": "invalid input"}
```

| Range     | Assigned to | Example codes |
|-----------|-------------|---------------|
| 0–999     | Daemon      | 1=UnknownCommand, 2=InvalidFormat, 3=ModuleNotFound |
| 1000–1999 | Calculator  | 1001=InvalidInput, 1002=Overflow, 1003=Timeout |
| 2000–2999 | Logger      | 2001=FileNotFound, 2002=PermissionDenied |
| 3000+     | Other       | Assign as needed |

The `message` field is optional — omit it if the code is self-explanatory.

---

## 19. Complete Module Lifecycle Example

```python
#!/usr/bin/env python3
"""Minimal DaemonV1 module example (Python)."""
import sys
import json

def send(msg):
    """Send a JSON message to the daemon via stdout."""
    print(json.dumps(msg), flush=True)

def recv():
    """Receive a JSON message from the daemon via stdin."""
    line = sys.stdin.readline()
    if not line:
        return None  # stdin closed
    return json.loads(line.strip())

def main():
    # 1. Receive init
    msg = recv()
    assert msg["cmd"] == "init"
    module_name = msg["module_name"]
    config = msg["config"]

    # 2. Initialize (load resources, etc.)
    send({"type": "log", "message": f"Initializing {module_name}..."})

    # 3. Signal ready
    send({"type": "init_complete"})

    # 4. Subscribe to own namespace
    send({"type": "subscribe_request", "topic": f"{module_name}.*"})

    # 5. Message loop
    while True:
        msg = recv()
        if msg is None:
            break  # stdin closed

        cmd = msg.get("cmd")

        if cmd == "command":
            cmd_id = msg["id"]
            # Process command...
            send({"type": "ack", "id": cmd_id})
            # Publish result
            send({
                "type": "publish",
                "topic": f"{module_name}.result",
                "metadata": {"id": cmd_id, "result": "done"}
            })

        elif cmd == "event":
            # Handle bus event
            pass

        elif cmd == "data_response":
            # Handle data read result
            pass

        elif cmd == "shutdown":
            send({"type": "log", "message": "Shutting down...", "level": "info"})
            break

    sys.exit(0)

if __name__ == "__main__":
    main()
```

### Timeline

```
Daemon                              Module (Python)
  |                                    |
  |--- spawn ./module.py ------------->|
  |--- init {module_name,config} ----->|
  |                                    | log "Initializing..."
  |<-- log ----------------------------|
  |<-- init_complete ------------------|  status → running
  |<-- subscribe_request --------------|
  |                                    |
  |--- command {id,action,...} ------->|
  |<-- ack {id} -----------------------|
  |<-- publish {topic,metadata} -------|  → bus delivers to subscribers
  |                                    |
  |--- shutdown {force,timeout} ------>|
  |<-- log "Shutting down..." ---------|
  |<-- process exit (code 0) ----------|  status → stopped
```
