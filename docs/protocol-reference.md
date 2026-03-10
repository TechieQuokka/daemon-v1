# Protocol Reference

Daemon V1의 모든 통신 프로토콜을 간결하게 정리한 레퍼런스입니다.

---

## Table of Contents

- [Module ↔ Daemon Protocol](#module--daemon-protocol)
- [Controller ↔ Daemon API](#controller--daemon-api)
- [Quick Reference](#quick-reference)

---

## Module ↔ Daemon Protocol

**Transport:** stdin/stdout JSON Lines
**Format:** 한 줄에 하나의 JSON 메시지

### Daemon → Module Messages

| Message | Parameters | Description |
|---------|------------|-------------|
| **init** | `module_name`, `config` | 모듈 초기화 |
| **command** | `id`, `payload` (free-form) | 컨트롤러 명령 전달 |
| **event** | `topic`, `data`, `publisher`, `timestamp` | Bus 이벤트 알림 |
| **shutdown** | `force`, `timeout` (optional) | 종료 요청 |
| **data_response** | `key`, `value`/`path` (optional) | Data Layer 조회 응답 |

**JSON 구조:**
```json
{
  "cmd": "init|command|event|shutdown|data_response",
  ...
}
```

---

### Module → Daemon Messages

| Message | Parameters | Description |
|---------|------------|-------------|
| **ack** | `id` | 명령 수신 확인 |
| **error** | `id`, `code`, `message` | 명령 실행 오류 |
| **publish** | `topic`, `metadata` | Bus에 이벤트 발행 |
| **subscribe_request** | `topic` | Bus 토픽 구독 ⚠️ |
| **unsubscribe_request** | `topic` | Bus 토픽 구독 해제 |
| **data_write** | `key`, `value`/`path` | Data Layer 쓰기 |
| **data_read** | `key` | Data Layer 읽기 |
| **data_delete** | `key` | Data Layer 삭제 |
| **log** | `message`, `level` | 로그 전송 |

**JSON 구조:**
```json
{
  "type": "ack|error|publish|subscribe_request|...",
  ...
}
```

⚠️ **subscribe_request 제한:**
- ✅ `system.*` - 시스템 이벤트
- ✅ `{module_id}.*` - 자신의 토픽
- ❌ `{other_module}.*` - 다른 모듈 (거부)

---

## Controller ↔ Daemon API

**Transport:** TCP :9000 JSON Lines
**Format:** Request-Response 패턴

### Request/Response Structure

**Request:**
```json
{
  "action": "module.start|data.get|...",
  "params": { ... },
  "id": "req-abc123"
}
```

**Response (Success):**
```json
{
  "id": "req-abc123",
  "success": true,
  "result": { ... }
}
```

**Response (Error):**
```json
{
  "id": "req-abc123",
  "success": false,
  "error": "Error message"
}
```

---

### Module Management API

| Action | Parameters | Response | Description |
|--------|------------|----------|-------------|
| **module.start** | `name`, `path`, `config` (opt) | `module_id` | 모듈 시작 |
| **module.stop** | `id`, `timeout` (opt) | `status` | 모듈 중지 |
| **module.list** | - | `modules[]` | 모듈 목록 |
| **health_check** | `module` | `module_id`, `status`, `pid` | 모듈 상태 확인 |

**Example:**
```json
// Request
{"action": "module.start", "params": {"name": "fibonacci", "path": "/bin/fib"}, "id": "1"}

// Response
{"id": "1", "success": true, "result": {"module_id": "fibonacci"}}
```

---

### Module Command API

| Action | Parameters | Response | Description |
|--------|------------|----------|-------------|
| **module.command** | `module`, `id`, `payload` (free-form) | `status` | 모듈에 명령 전송 |

**Example:**
```json
// Request
{"action": "module.command", "params": {"module": "fibonacci", "id": "cmd-1", "action": "calculate", "n": 10}, "id": "2"}

// Response
{"id": "2", "success": true, "result": {"status": "sent"}}
```

⚠️ **주의:** 실제 결과는 Module이 Data Layer에 저장하므로 polling 필요

---

### Message Bus API

| Action | Parameters | Response | Description |
|--------|------------|----------|-------------|
| **bus.publish** | `topic`, `data` (opt) | `status` | Bus에 이벤트 발행 |
| **bus.subscribe** | `topic`, `timeout` (opt) | `topic`, `data`, `publisher`, `timestamp` OR `timeout: true` | Bus 이벤트 구독 (Long Polling) |

**Example (bus.publish):**
```json
// Request
{"action": "bus.publish", "params": {"topic": "fibonacci.command", "data": {"action": "calc"}}, "id": "3"}

// Response
{"id": "3", "success": true, "result": {"status": "published"}}
```

**Example (bus.subscribe):**
```json
// Request
{"action": "bus.subscribe", "params": {"topic": "fibonacci.result", "timeout": 30000}, "id": "4"}

// Response (event received)
{"id": "4", "success": true, "result": {"topic": "fibonacci.result", "data": {"n": 10, "result": 55}, "publisher": "fibonacci", "timestamp": 1234567890}}

// Response (timeout)
{"id": "4", "success": true, "result": {"topic": "fibonacci.result", "data": null, "timeout": true}}
```

⚠️ **주의:** `bus.subscribe`는 Long Polling 방식 (최대 timeout까지 대기, 이벤트 도착 시 즉시 반환)

---

### Data Layer API

| Action | Parameters | Response | Description |
|--------|------------|----------|-------------|
| **data.get** | `key` | `key`, `value`/`path` | 값 조회 |
| **data.set** | `key`, `value`/`path` | `key`, `status` | 값 저장 |
| **data.delete** | `key` | `key`, `deleted` | 값 삭제 |
| **data.list** | - | `keys[]` | 키 목록 조회 |

**Example:**
```json
// Request (get)
{"action": "data.get", "params": {"key": "fibonacci.result.123"}, "id": "4"}

// Response (value exists)
{"id": "4", "success": true, "result": {"key": "fibonacci.result.123", "value": {"n": 10, "result": 55}}}

// Response (value not found)
{"id": "4", "success": true, "result": {"key": "fibonacci.result.123", "value": null}}
```

**Inline vs File:**
- **Inline:** `{"key": "k", "value": {...}}`  → 작은 데이터
- **File:** `{"key": "k", "path": "/data/..."}` → 대용량 데이터

---

### Daemon Management API

| Action | Parameters | Response | Description |
|--------|------------|----------|-------------|
| **daemon.status** | - | `modules`, `subscribers`, `data_keys`, `status` | Daemon 상태 |
| **daemon.shutdown** | - | `status` | Daemon 종료 |

**Example:**
```json
// Request
{"action": "daemon.status", "params": {}, "id": "5"}

// Response
{"id": "5", "success": true, "result": {"modules": 3, "subscribers": 5, "data_keys": 12, "status": "running"}}
```

---

## Quick Reference

### Message Direction Flow

```
┌─────────────┐                      ┌──────────────┐
│ Controller  │                      │   Module     │
└──────┬──────┘                      └──────┬───────┘
       │                                    │
       │ TCP :9000                          │ stdin/stdout
       │                                    │
┌──────▼────────────────────────────────────▼──────┐
│               Daemon Core                        │
│                                                  │
│  ┌─────────┐  ┌──────────┐  ┌────────────────┐ │
│  │   Bus   │  │Data Layer│  │Module Manager  │ │
│  └─────────┘  └──────────┘  └────────────────┘ │
└──────────────────────────────────────────────────┘
```

---

### Communication Patterns

| Pattern | Transport | Use Case | Example |
|---------|-----------|----------|---------|
| **Controller → Daemon → Module** | TCP → stdin | 동기 명령 | `module.command` → `command` → `ack` |
| **Controller → Bus → Module** | TCP → Bus → stdin | 비동기 이벤트 | `bus.publish` → `event` |
| **Module → Data Layer → Controller** | stdout → memory → TCP | 결과 반환 | `data_write` → `data.get` |
| **Module → Bus → Module** | stdout → Bus → stdin | ❌ 제한됨 | 동일 namespace만 |

---

### Data Size Guidelines

| Size | Method | Transport | Example |
|------|--------|-----------|---------|
| **< 1KB** | module.command + Data Layer | JSON inline | `{"result": 55}` |
| **1KB - 1MB** | module.command + Data Layer | JSON inline | `{"users": [...]}` |
| **1MB - 100MB** | Bus + Data Layer (file path) | File reference | `{"path": "/data/result.json"}` |
| **> 100MB** | Bus + Data Layer (file path) | File reference | `{"path": "/data/video.mp4"}` |

---

### Topic Naming Convention

| Prefix | Owner | Subscribe | Publish | Example |
|--------|-------|-----------|---------|---------|
| **system.*** | Daemon | All ✅ | Controller | `system.shutdown` |
| **{module}.*** | Module | Self ✅ | Self + Controller | `fibonacci.command` |
| **{module}.command** | Module | Self ✅ | Controller | `fibonacci.command` |
| **{module}.result** | Module | ❌ | Self | `fibonacci.result` |
| **{module}.progress** | Module | ❌ | Self | `fibonacci.progress` |

**Rule:**
- Module은 `system.*` 또는 `{self}.*`만 구독 가능
- 다른 Module 토픽 구독 → 자동 거부

---

### Error Codes

| Range | Owner | Example |
|-------|-------|---------|
| **0000-0999** | Daemon | `1` (Unknown command), `2` (Invalid format), `3` (Module not found) |
| **1000-1999** | Module 1 | `1001` (Invalid input), `1002` (Overflow) |
| **2000-2999** | Module 2 | `2001` (File not found), `2002` (Permission denied) |
| **5000-5999** | Controller | `5001` (Internal error), `5002` (Module start failed) |

---

### Common Workflows

#### 1. Start Module & Execute Command

```
Controller                 Daemon                   Module
    │                         │                         │
    │ module.start ──────────►│                         │
    │◄──────────── module_id  │                         │
    │                         │ init ──────────────────►│
    │                         │◄─────────────────── ack │
    │                         │                         │
    │ module.command ────────►│                         │
    │                         │ command ───────────────►│
    │                         │◄─────────────────── ack │
    │                         │                         │
    │                         │         (processing...) │
    │                         │                         │
    │                         │◄─── data_write ─────────│
    │                         │ (Data Layer 저장)       │
    │                         │                         │
    │ data.get (polling) ────►│                         │
    │◄───────────── result    │                         │
```

#### 2. Bus Event Flow

```
Controller                 Daemon                   Module
    │                         │                         │
    │                         │ subscribe_request ─────►│
    │                         │◄───── (validated)       │
    │                         │ (Registry 등록)         │
    │                         │                         │
    │ bus.publish ───────────►│                         │
    │                         │ (Bus 전파)              │
    │                         │ event ─────────────────►│
    │                         │                         │
    │                         │         (processing...) │
    │                         │                         │
    │                         │◄─── publish ────────────│
    │                         │ (Bus 재전파)            │
```

#### 3. Large File Processing

```
Module A                   Daemon                   Module B
    │                         │                         │
    │ publish ───────────────►│                         │
    │ (topic: video.raw,      │                         │
    │  path: /data/4k.mp4)    │                         │
    │                         │                         │
    │                         │ event ─────────────────►│
    │                         │ (path 전달)             │
    │                         │                         │
    │                         │         (파일 직접 읽기)│
    │                         │                         │
    │                         │◄─── publish ────────────│
    │                         │ (topic: video.done)     │
```

---

## Summary

### Protocol Characteristics

| Aspect | Module ↔ Daemon | Controller ↔ Daemon |
|--------|-----------------|---------------------|
| **Transport** | stdin/stdout | TCP :9000 |
| **Format** | JSON Lines | JSON Lines |
| **Pattern** | Bidirectional streaming | Request-Response |
| **Initiated by** | Both | Controller only |
| **State** | Stateful (session) | Stateless (per request) |
| **Lifecycle** | Daemon manages | Controller manages |

### Design Principles

1. **Module Isolation**: Module은 서로 직접 통신 불가 (namespace 검증)
2. **Controller Orchestration**: Controller가 workflow 제어
3. **Daemon = Infrastructure**: 정책 없이 메커니즘만 제공
4. **Volatile Storage**: Data Layer는 메모리만 (재시작 시 초기화)
5. **FIFO Guarantee**: Bus는 순차 처리 (race condition 없음)

---

## See Also

- [Developer Guide](dev-guide.md) - 상세 가이드 및 예제
- [README](../README.md) - 프로젝트 개요
