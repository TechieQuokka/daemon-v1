# Daemon V1

A stable, extensible Rust-based daemon with process-isolated modules, central message bus, volatile data storage, and TCP-based controller interface.

## Architecture

```
┌─────────────┐
│  Controller │ (TCP :9000)
└──────┬──────┘
       │
┌──────▼──────────────────────────────────────┐
│              Daemon Core                     │
│                                              │
│  ┌──────────────┐  ┌──────────────────────┐ │
│  │ Message Bus  │  │    Data Layer        │ │
│  │ (FIFO, pub/  │  │ (K/V + SIEVE)        │ │
│  │  sub)        │  │                      │ │
│  └──────────────┘  └──────────────────────┘ │
└──────┬───────┬───────┬───────────────────────┘
       │       │       │
   ┌───▼──┐ ┌──▼──┐ ┌──▼──┐
   │Module│ │Module│ │Module│ (stdin/stdout JSON)
   └──────┘ └─────┘ └─────┘
```

## Key Features

### 1. Stability First

- **FIFO Message Ordering**: Sequential bus processor guarantees message order
- **Lock-Based Concurrency**: Data layer uses RwLock for safe concurrent access
- **Sequential Processing**: No race conditions in message routing

### 2. Simple Infrastructure

- Daemon provides only infrastructure (communication, storage, routing)
- No business logic in daemon
- Modules are independent processes
- Controller manages policies and orchestration

### 3. Extensibility

- **Dynamic Module Loading**: Add/remove modules at runtime
- **Wildcard Subscriptions**: Subscribe to patterns like `user.*` or `calc.#`
- **SIEVE Eviction**: Automatic memory management with state-of-the-art algorithm
- **Volatile Storage**: Memory-only data layer (persistence is Controller's role)

## Protocol Specifications

### Module ↔ Daemon Protocol (stdin/stdout JSON)

#### Daemon → Module

```json
// Initialize module
{
  "cmd": "init",
  "module_name": "calculator",
  "config": {
    "data_layer_path": "/data_layer"
  }
}

// Send command (free-form payload)
{
  "cmd": "command",
  "id": "req-123",
  "action": "calculate",
  "n": 30
}

// Event notification
{
  "cmd": "event",
  "topic": "calc.done",
  "data": {"result": 832040},
  "publisher": "calculator",
  "timestamp": 1234567890
}

// Shutdown request
{
  "cmd": "shutdown",
  "force": false,
  "timeout": 5000
}

// Data response
{
  "cmd": "data_response",
  "key": "count",
  "value": 123
}
```

#### Module → Daemon

```json
// Acknowledge command
{
  "type": "ack",
  "id": "req-123"
}

// Error response
{
  "type": "error",
  "id": "req-123",
  "code": 1002,
  "message": "integer overflow"
}

// Publish event
{
  "type": "publish",
  "topic": "calc.done",
  "metadata": {"key": "fib_30", "elapsed_ms": 0.009}
}

// Subscribe to topic
{
  "type": "subscribe_request",
  "topic": "calc.*"
}

// Write data (inline)
{
  "type": "data_write",
  "key": "count",
  "value": 123
}

// Write data (file reference)
{
  "type": "data_write",
  "key": "sales",
  "path": "/data_layer/sales_20260308.json"
}

// Read data
{
  "type": "data_read",
  "key": "count"
}

// Log message
{
  "type": "log",
  "message": "Calculation completed",
  "level": "info"
}
```

### Controller ↔ Daemon Protocol (TCP :9000 JSON)

```json
// Request
{
  "action": "module.start",
  "params": {
    "name": "calculator",
    "config": {...}
  },
  "id": "req-123"
}

// Response
{
  "id": "req-123",
  "success": true,
  "result": {
    "module_id": "calc-abc123"
  }
}

// Error response
{
  "id": "req-123",
  "success": false,
  "error": "Module not found"
}
```

#### Available Actions

**Module Management:**

- `module.start` - Start a new module
- `module.stop` - Stop a running module
- `module.list` - List all modules
- `health_check` - Check module health

**Module Commands:**

- `module.command` - Send command to module

**Data Layer:**

- `data.get` - Read from data layer
- `data.set` - Write to data layer
- `data.delete` - Delete from data layer
- `data.list` - List all keys

**Message Bus:**

- `bus.publish` - Publish event to bus

**Daemon:**

- `daemon.status` - Get daemon status
- `daemon.shutdown` - Shutdown daemon

## Message Bus

### Topic Format

Topics use hierarchical structure with dot separators:

```
module.category.action
```

Examples:

- `calculator.task.done`
- `logger.level.error`
- `monitor.system.tick`

### Wildcard Subscriptions

- `*` - Matches exactly one segment
  - `calculator.*` matches `calculator.done` but not `calculator.task.done`
  - `*.created` matches `user.created` and `post.created`

- `#` - Matches zero or more segments
  - `calculator.#` matches `calculator.done` and `calculator.task.done`
  - `#.created` matches `created`, `user.created`, `user.profile.created`
  - `#` matches all events

### Ordering Guarantee

Messages are delivered in FIFO order (first published, first received).

### Overflow Handling

When bus buffer is full (default: 10,000 events):

- New events are **rejected**
- FIFO order is preserved
- Module receives error response

## Data Layer

### Storage Types

**Small data (inline):**

```json
{
  "type": "data_write",
  "key": "count",
  "value": 123
}
```

**Large data (file reference):**

```json
{
  "type": "data_write",
  "key": "sales",
  "path": "/data_layer/sales_20260308.json"
}
```

### Characteristics

- **Volatile**: Memory-only storage (lost on daemon restart)
- **Capacity**: Default 10,000 keys (configurable)
- **Eviction**: SIEVE algorithm (NSDI'24) automatically removes old entries
- **Concurrency**: Thread-safe with RwLock

### SIEVE Eviction

When cache is full:

1. SIEVE algorithm identifies least valuable entry
2. Entry is automatically removed
3. New entry is stored
4. No module intervention needed

## Error Codes

Error codes are organized by module range:

- **0000-0999**: Daemon common errors
  - `1`: Unknown command
  - `2`: Invalid format
  - `3`: Module not found

- **1000-1999**: Calculator module
  - `1001`: Invalid input
  - `1002`: Overflow
  - `1003`: Timeout

- **2000-2999**: Logger module
  - `2001`: File not found
  - `2002`: Permission denied

- **3000-3999**: Monitor module (reserved)

## Configuration

### Config File (TOML)

```toml
# Daemon configuration
ipc_address = "127.0.0.1:9000"

[bus]
max_events = 10000

[storage]
max_keys = 10000
data_layer_path = "/data_layer"
```

## Building and Running

### Build

```bash
cargo build --release
```

### Run

```bash
# With default configuration
./target/release/daemon_v1

# With custom configuration
./target/release/daemon_v1 --config config.toml
```

### Run Tests

```bash
# Unit tests
cargo test

# Integration tests
cargo test --test '*'

# All tests with logging
cargo test -- --nocapture
```

## Module Development

See `examples/modules/` for reference implementations in:

- Rust
- Python
- Node.js

### Module Requirements

1. Read JSON lines from stdin
2. Write JSON lines to stdout
3. Handle `init`, `command`, `event`, `shutdown` messages
4. Send `ack` for all commands
5. Use proper error codes

## Project Status

### Implemented ✅

- [x] Error types and handling
- [x] Protocol definitions (Module, Controller)
- [x] JSON line codec
- [x] SIEVE cache algorithm
- [x] Thread-safe data layer
- [x] Topic pattern matching (wildcards)
- [x] Subscription registry
- [x] Sequential message bus processor
- [x] Configuration management
- [x] Basic daemon structure

### In Progress 🚧

- [ ] Module process management
- [ ] IPC server (TCP)
- [ ] Lifecycle management
- [ ] Dynamic module loading
- [ ] Health checks

### Planned 📋

- [ ] Example modules
- [ ] Controller client library
- [ ] Performance benchmarks
- [ ] Docker deployment
- [ ] systemd integration
- [ ] Monitoring/metrics

## Architecture Decisions

### Why Sequential Processing?

**Stability over throughput**: FIFO guarantee prevents race conditions and makes system behavior predictable. Performance is optimized through async I/O, not parallel message processing.

### Why Volatile Storage?

**Separation of concerns**: Daemon provides infrastructure, Controller manages business logic including persistence. This keeps the daemon simple and focused.

### Why Process Isolation?

**Independence**: Each module runs in separate process. Crash doesn't affect daemon or other modules. Different languages can be used.

### Why SIEVE?

**Efficiency**: Recent research (NSDI'24) shows SIEVE has lower miss ratio than LRU while being simpler than ARC. Automatic eviction reduces memory management burden on modules.

## License

[License information here]
