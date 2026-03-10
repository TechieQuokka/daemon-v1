# Daemon V1 Developer Guide

Daemon V1의 아키텍처, API, 그리고 개발 가이드입니다.

---

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Internal Flow Diagrams](#internal-flow-diagrams)
- [Daemon API Reference](#daemon-api-reference)
- [Decision Guide](#decision-guide)
- [Best Practices](#best-practices)
- [Examples](#examples)

---

## Overview

Daemon V1은 **불변 인프라 레이어**로, Module과 Controller에게 통신 및 데이터 공유 메커니즘을 제공합니다.

### Design Philosophy

```
┌─────────────────────────────────────────────────────┐
│  Daemon (불변)                                       │
│  - 메커니즘만 제공 (Bus, Data Layer, IPC)             │
│  - 정책은 제공하지 않음                                │
└─────────────────────────────────────────────────────┘
           ▲                           ▲
           │                           │
┌──────────┴──────────┐     ┌──────────┴──────────┐
│  Controller (유연)  │     │  Module (유연)      │
│  - 정책 결정        │     │  - 비즈니스 로직    │
│  - Orchestration    │     │  - 도메인 로직      │
└─────────────────────┘     └─────────────────────┘
```

**핵심 원칙:**

- **Daemon**: 도구만 제공, 어떻게 쓸지는 개발자가 결정
- **Controller**: 상황에 맞게 도구 선택 (command vs Bus)
- **Module**: 독립적으로 동작, 서로 직접 통신 불가

---

## Architecture

### System Structure

```
┌─────────────────────────────────────────────────────────────────┐
│                          App Layer                              │
│                   (비즈니스 로직, UI, API)                       │
└───────────────────────────────┬─────────────────────────────────┘
                                │ stdin/stdout JSON
┌───────────────────────────────▼─────────────────────────────────┐
│                        Controller                               │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  - Orchestration (workflow 제어)                         │  │
│  │  - Policy (어떤 Module에게 무엇을 시킬지)                │  │
│  │  - 도구 선택 (command vs Bus)                            │  │
│  └──────────────────────────────────────────────────────────┘  │
└───────────────────────────────┬─────────────────────────────────┘
                                │ TCP :9000 JSON
┌───────────────────────────────▼─────────────────────────────────┐
│                      Daemon Core (불변)                         │
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌────────────────┐ │
│  │  Message Bus    │  │  Data Layer     │  │ Module Manager │ │
│  │  (FIFO Queue)   │  │  (SIEVE Cache)  │  │ (Lifecycle)    │ │
│  │                 │  │                 │  │                │ │
│  │  • Pub/Sub      │  │  • K/V Store    │  │ • Start/Stop   │ │
│  │  • Wildcards    │  │  • Eviction     │  │ • Health       │ │
│  │  • Sequential   │  │  • Thread-safe  │  │ • Command      │ │
│  └─────────────────┘  └─────────────────┘  └────────────────┘ │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │            IPC Server (TCP :9000)                        │  │
│  │  • Request-Response matching                            │  │
│  │  • Action routing                                       │  │
│  │  • Error handling                                       │  │
│  └──────────────────────────────────────────────────────────┘  │
└──────┬──────────────────┬──────────────────┬───────────────────┘
       │ stdin/stdout     │ stdin/stdout     │ stdin/stdout
       │ JSON             │ JSON             │ JSON
┌──────▼──────┐    ┌──────▼──────┐    ┌──────▼──────┐
│  Module A   │    │  Module B   │    │  Module C   │
│ (Fibonacci) │    │   (Video)   │    │  (Logger)   │
│             │    │             │    │             │
│ • 독립 실행  │    │ • 독립 실행   │    │ • 독립 실행  │
│ • 격리됨     │    │ • 격리됨     │    │ • 격리됨     │
│ • 언어무관   │    │ • 언어무관    │    │ • 언어무관   │
└─────────────┘    └─────────────┘    └─────────────┘
```

### Core Components

#### 1. Message Bus

- **용도**: 비동기 이벤트 스트림
- **특징**:
  - FIFO 순서 보장
  - Pub/Sub 패턴
  - Wildcard 지원 (`*`, `#`)
  - Sequential 처리 (race condition 없음)
  - **Module 구독 제한**: Module은 `system.*` 또는 `{module_id}.*`만 구독 가능 (자동 검증)

#### 2. Data Layer

- **용도**: Module 간 데이터 공유
- **특징**:
  - Key-Value 저장소
  - SIEVE 자동 eviction (NSDI'24)
  - Thread-safe (RwLock)
  - Volatile (메모리만)

#### 3. Module Manager

- **용도**: Module lifecycle 관리
- **특징**:
  - Process 격리
  - stdin/stdout JSON 통신
  - Health monitoring
  - Graceful shutdown

---

## Internal Flow Diagrams

### 1. module.command Flow (동기 요청-응답)

```
┌─────────────┐
│ Controller  │
└──────┬──────┘
       │ TCP :9000
       │ {"action": "module.command", "params": {...}}
       ▼
┌──────────────────────────────────────────┐
│          Daemon Core                     │
│                                          │
│  ┌────────────────────────────────────┐ │
│  │   IPC Handler (TCP Server)         │ │
│  │   - Request ID 매칭                │ │
│  │   - Action 라우팅                  │ │
│  └──────────────┬─────────────────────┘ │
│                 │                        │
│  ┌──────────────▼─────────────────────┐ │
│  │   Module Manager                   │ │
│  │   - Module 찾기                    │ │
│  │   - Command 메시지 생성            │ │
│  └──────────────┬─────────────────────┘ │
│                 │                        │
└─────────────────┼────────────────────────┘
                  │ stdin
                  │ {"cmd": "command", "id": "req-123", "action": "calc"}
                  ▼
           ┌──────────────┐
           │    Module    │
           │  (Process)   │
           └──────┬───────┘
                  │ stdout
                  │ {"type": "ack", "id": "req-123"}
                  ▼
┌─────────────────────────────────────────┐
│          Daemon Core                    │
│                                         │
│  ┌────────────────────────────────────┐│
│  │   Module Manager                   ││
│  │   - ACK 수신                       ││
│  │   - 결과 대기 (비동기)             ││
│  └────────────────────────────────────┘│
└─────────────────────────────────────────┘

  (Module이 처리 완료 후 Data Layer에 저장)
                  │
                  ▼
           Data Layer에 결과 저장
                  │
                  ▼
  Controller가 polling으로 결과 확인
```

### 2. Bus Flow (비동기 이벤트)

```
┌─────────────┐
│ Controller  │
└──────┬──────┘
       │ TCP
       │ {"action": "bus.publish", "params": {"topic": "fib.cmd"}}
       ▼
┌──────────────────────────────────────────────────────┐
│                 Daemon Core                          │
│                                                      │
│  ┌────────────────────────────────────────────────┐ │
│  │  IPC Handler                                   │ │
│  └──────────────┬─────────────────────────────────┘ │
│                 │                                    │
│  ┌──────────────▼─────────────────────────────────┐ │
│  │  Message Bus (FIFO Queue)                      │ │
│  │                                                 │ │
│  │  ┌─────────────────────────────────────────┐  │ │
│  │  │ Sequential Processor                    │  │ │
│  │  │  while msg = queue.recv():              │  │ │
│  │  │    for subscriber in registry:          │  │ │
│  │  │      if match(msg.topic, pattern):      │  │ │
│  │  │        subscriber.send(msg)             │  │ │
│  │  └─────────────────────────────────────────┘  │ │
│  │                                                 │ │
│  │  ┌─────────────────────────────────────────┐  │ │
│  │  │ Subscription Registry                   │  │ │
│  │  │  "fib.cmd" → [Module A channel]         │  │ │
│  │  │  "fib.result" → [Controller channel]    │  │ │
│  │  │  "system.*" → [Module B, C channels]    │  │ │
│  │  └─────────────────────────────────────────┘  │ │
│  └──────────────┬──────────────────────────────┬──┘ │
│                 │                              │    │
└─────────────────┼──────────────────────────────┼────┘
                  │                              │
        ┌─────────▼────────┐          ┌─────────▼────────┐
        │   Module A       │          │   Module B       │
        │  (subscribe      │          │  (subscribe      │
        │   "fib.cmd")     │          │   "system.*")    │
        └─────────┬────────┘          └──────────────────┘
                  │ stdin
                  │ {"cmd": "event", "topic": "fib.cmd", "data": {...}}
                  ▼
         Module 처리 후 결과 publish
                  │ stdout
                  │ {"type": "publish", "topic": "fib.result", "metadata": {...}}
                  ▼
┌─────────────────────────────────────────────────────┐
│                 Message Bus                         │
│     (다시 순차 처리 → 구독자에게 전달)               │
└─────────────────────────────────────────────────────┘
```

### 3. Data Layer Flow

```
┌─────────────┐                    ┌─────────────┐
│  Module A   │                    │  Module B   │
└──────┬──────┘                    └──────┬──────┘
       │ stdout                           │ stdout
       │ {"type": "data_write",           │ {"type": "data_read",
       │  "key": "result.123",             │  "key": "result.123"}
       │  "value": {...}}                 │
       ▼                                  ▼
┌────────────────────────────────────────────────────┐
│              Daemon Core                           │
│                                                    │
│  ┌──────────────────────────────────────────────┐ │
│  │  Module Manager (Message Handler)            │ │
│  │   - data_write 처리                          │ │
│  │   - data_read 처리                           │ │
│  └──────────────┬───────────────────────────────┘ │
│                 │                                  │
│  ┌──────────────▼───────────────────────────────┐ │
│  │  Data Layer (Thread-safe)                    │ │
│  │                                               │ │
│  │  RwLock<SieveCache>                          │ │
│  │  ┌─────────────────────────────────────────┐ │ │
│  │  │ HashMap + VecDeque                      │ │ │
│  │  │                                         │ │ │
│  │  │ "result.123" → {n: 10, result: 55}     │ │ │
│  │  │ "video.path" → "/data/4k.mp4"          │ │ │
│  │  │ "dataset.url" → {...}                  │ │ │
│  │  │                                         │ │ │
│  │  │ SIEVE Eviction (자동)                  │ │ │
│  │  │  - visited bit tracking                │ │ │
│  │  │  - hand pointer sweeping               │ │ │
│  │  └─────────────────────────────────────────┘ │ │
│  └──────────────┬───────────────────────────────┘ │
│                 │                                  │
└─────────────────┼──────────────────────────────────┘
                  │ stdin
                  │ {"cmd": "data_response", "key": "result.123", "value": {...}}
                  ▼
           ┌─────────────┐
           │  Module B   │
           └─────────────┘
```

---

## Daemon API Reference

Controller가 Daemon과 통신할 때 사용하는 API입니다.

### Protocol

**Request:**

```json
{
  "action": "module.start",
  "params": {
    "name": "fibonacci",
    "path": "/path/to/module",
    "config": {}
  },
  "id": "req-abc123"
}
```

**Response:**

```json
{
  "id": "req-abc123",
  "success": true,
  "result": {
    "module_id": "fibonacci"
  }
}
```

**Error Response:**

```json
{
  "id": "req-abc123",
  "success": false,
  "error": "Module not found"
}
```

---

### Module Management

#### `module.start`

모듈을 시작합니다.

**Parameters:**

- `name` (string, required): 모듈 식별자
- `path` (string, required): 모듈 실행 파일 경로
- `config` (object, optional): 모듈 초기화 설정

**Request:**

```json
{
  "action": "module.start",
  "params": {
    "name": "fibonacci",
    "path": "/usr/local/bin/fibonacci",
    "config": {
      "max_n": 100
    }
  },
  "id": "req-1"
}
```

**Response:**

```json
{
  "id": "req-1",
  "success": true,
  "result": {
    "module_id": "fibonacci"
  }
}
```

---

#### `module.stop`

모듈을 중지합니다.

**Parameters:**

- `id` (string, required): 모듈 식별자
- `timeout` (number, optional): 타임아웃 (ms, default: 5000)

**Request:**

```json
{
  "action": "module.stop",
  "params": {
    "id": "fibonacci",
    "timeout": 5000
  },
  "id": "req-2"
}
```

**Response:**

```json
{
  "id": "req-2",
  "success": true,
  "result": {
    "status": "stopped"
  }
}
```

---

#### `module.list`

실행 중인 모듈 목록을 조회합니다.

**Parameters:** 없음

**Request:**

```json
{
  "action": "module.list",
  "params": {},
  "id": "req-3"
}
```

**Response:**

```json
{
  "id": "req-3",
  "success": true,
  "result": {
    "modules": [
      {
        "id": "fibonacci",
        "status": "running",
        "pid": 12345,
        "subscriptions": ["system.shutdown", "fibonacci.command"]
      }
    ]
  }
}
```

---

#### `health_check`

모듈 상태를 확인합니다.

**Parameters:**

- `module` (string, required): 모듈 식별자

**Request:**

```json
{
  "action": "health_check",
  "params": {
    "module": "fibonacci"
  },
  "id": "req-4"
}
```

**Response:**

```json
{
  "id": "req-4",
  "success": true,
  "result": {
    "module_id": "fibonacci",
    "status": "running",
    "pid": 12345
  }
}
```

---

### Module Command

#### `module.command`

모듈에게 명령을 전송합니다 (동기).

**Parameters:**

- `module` (string, required): 모듈 식별자
- `id` (string, required): 커맨드 ID
- 기타 필드: 모듈별로 다름 (free-form payload)

**Request:**

```json
{
  "action": "module.command",
  "params": {
    "module": "fibonacci",
    "id": "cmd-123",
    "action": "calculate",
    "n": 10
  },
  "id": "req-5"
}
```

**Response:**

```json
{
  "id": "req-5",
  "success": true,
  "result": {
    "status": "sent"
  }
}
```

**Note:** 실제 결과는 Module이 Data Layer에 저장하므로, Controller가 polling으로 확인해야 합니다.

---

### Message Bus

#### `bus.publish`

Bus에 이벤트를 발행합니다 (비동기).

**Parameters:**

- `topic` (string, required): 토픽 이름
- `data` (object, optional): 이벤트 데이터

**Request:**

```json
{
  "action": "bus.publish",
  "params": {
    "topic": "fibonacci.command",
    "data": {
      "action": "calculate_range",
      "start": 0,
      "count": 10
    }
  },
  "id": "req-6"
}
```

**Response:**

```json
{
  "id": "req-6",
  "success": true,
  "result": {
    "status": "published"
  }
}
```

---

#### `bus.subscribe`

Bus 토픽을 구독하고 subscriber ID를 발급받습니다.

**Parameters:**

- `topic` (string, required): 구독할 토픽 패턴 (wildcards 지원)

**Request:**

```json
{
  "action": "bus.subscribe",
  "params": {
    "topic": "fibonacci.result"
  },
  "id": "req-7"
}
```

**Response:**

```json
{
  "id": "req-7",
  "success": true,
  "result": {
    "subscriber_id": "controller:550e8400-e29b-41d4-a716-446655440000"
  }
}
```

---

#### `bus.recv`

구독한 토픽의 이벤트를 수신합니다 (Long Polling).

**Parameters:**

- `subscriber_id` (string, required): `bus.subscribe`로 발급받은 ID
- `timeout` (number, optional): 타임아웃 (밀리초, 기본값: 30000)

**Request:**

```json
{
  "action": "bus.recv",
  "params": {
    "subscriber_id": "controller:550e8400-e29b-41d4-a716-446655440000",
    "timeout": 30000
  },
  "id": "req-8"
}
```

**Response (이벤트 수신):**

```json
{
  "id": "req-8",
  "success": true,
  "result": {
    "topic": "fibonacci.result",
    "data": {
      "n": 10,
      "result": 55
    },
    "timestamp": 1234567890
  }
}
```

**Response (타임아웃):**

```json
{
  "id": "req-8",
  "success": true,
  "result": {
    "timeout": true
  }
}
```

**동작 방식:**

1. Controller가 `bus.subscribe`로 구독 등록 후 `subscriber_id` 발급
2. `bus.recv`로 이벤트 대기 (Long Polling)
3. 이벤트 도착 시 즉시 응답 반환 (Fast Path)
4. 타임아웃 시 `timeout: true`와 함께 응답 반환
5. 여러 이벤트를 받으려면 `bus.recv` 반복 호출

**활용 사례:**

- Module 결과 실시간 수신 (폴링 대체)
- 스트리밍 데이터 전송 (진행률, 부분 결과)
- 이벤트 기반 워크플로우

**Note:** Long Polling 방식이므로 여러 이벤트를 받으려면 `bus.recv` 반복 호출 필요

---

### Data Layer

#### `data.get`

Data Layer에서 값을 조회합니다.

**Parameters:**

- `key` (string, required): 데이터 키

**Request:**

```json
{
  "action": "data.get",
  "params": {
    "key": "fibonacci.result.req-123"
  },
  "id": "req-7"
}
```

**Response (값 있음):**

```json
{
  "id": "req-7",
  "success": true,
  "result": {
    "key": "fibonacci.result.req-123",
    "value": {
      "n": 10,
      "result": 55,
      "elapsed_ms": 0.5
    }
  }
}
```

**Response (값 없음):**

```json
{
  "id": "req-7",
  "success": true,
  "result": {
    "key": "fibonacci.result.req-123",
    "value": null
  }
}
```

---

#### `data.set`

Data Layer에 값을 저장합니다.

**Parameters:**

- `key` (string, required): 데이터 키
- `value` (any, optional): 인라인 값
- `path` (string, optional): 파일 경로 (대용량)

**Note:** `value` 또는 `path` 중 하나는 필수입니다.

**Request (인라인):**

```json
{
  "action": "data.set",
  "params": {
    "key": "config.settings",
    "value": {
      "theme": "dark",
      "lang": "ko"
    }
  },
  "id": "req-8"
}
```

**Request (파일):**

```json
{
  "action": "data.set",
  "params": {
    "key": "video.processed",
    "path": "/data/output_4k.mp4"
  },
  "id": "req-9"
}
```

**Response:**

```json
{
  "id": "req-8",
  "success": true,
  "result": {
    "key": "config.settings",
    "status": "set"
  }
}
```

---

#### `data.delete`

Data Layer에서 값을 삭제합니다.

**Parameters:**

- `key` (string, required): 데이터 키

**Request:**

```json
{
  "action": "data.delete",
  "params": {
    "key": "temp.cache"
  },
  "id": "req-10"
}
```

**Response:**

```json
{
  "id": "req-10",
  "success": true,
  "result": {
    "key": "temp.cache",
    "deleted": true
  }
}
```

---

#### `data.list`

Data Layer의 모든 키 목록을 조회합니다.

**Parameters:** 없음

**Request:**

```json
{
  "action": "data.list",
  "params": {},
  "id": "req-11"
}
```

**Response:**

```json
{
  "id": "req-11",
  "success": true,
  "result": {
    "keys": ["fibonacci.result.req-123", "config.settings", "video.processed"]
  }
}
```

---

### Daemon Management

#### `daemon.status`

Daemon 상태를 조회합니다.

**Parameters:** 없음

**Request:**

```json
{
  "action": "daemon.status",
  "params": {},
  "id": "req-12"
}
```

**Response:**

```json
{
  "id": "req-12",
  "success": true,
  "result": {
    "modules": 3,
    "subscribers": 5,
    "data_keys": 12,
    "status": "running"
  }
}
```

---

#### `daemon.shutdown`

Daemon을 종료합니다.

**Parameters:** 없음

**Request:**

```json
{
  "action": "daemon.shutdown",
  "params": {},
  "id": "req-13"
}
```

**Response:**

```json
{
  "id": "req-13",
  "success": true,
  "result": {
    "status": "shutting_down"
  }
}
```

---

## Decision Guide

### module.command vs Bus: 언제 무엇을 사용할까?

#### ✅ module.command 사용 (동기 요청-응답)

**언제:**

- 작은 JSON 데이터 (< 1MB)
- 즉시 결과 필요
- 1:1 통신
- 진행 상황 불필요

**예시:**

```python
# 단일 값 계산
response = daemon.command('fibonacci', {
    'action': 'calculate',
    'n': 10
})

# 설정 조회
response = daemon.command('config_service', {
    'action': 'get_setting',
    'key': 'theme'
})

# User 조회 (적은 수)
response = daemon.command('user_service', {
    'action': 'get_users_by_group',
    'group_id': 'admin',
    'limit': 100
})
```

---

#### ✅ Bus 사용 (비동기 이벤트)

**언제:**

- 비동기 처리 필요
- 스트리밍 데이터
- 진행 상황 알림
- 대용량 데이터 (파일 경로 전송)
- 1:N 또는 N:N 통신 가능성

**예시:**

```python
# 1. 스트리밍 결과
controller.publish('fibonacci.command', {
    'action': 'calculate_range',
    'start': 0,
    'count': 10000
})
# Module이 배치로 발행:
# fibonacci.stream → {index: 0, value: 0}
# fibonacci.stream → {index: 1, value: 1}
# ...

# 2. 대용량 파일 처리
controller.publish('video.process', {
    'input': '/data/raw_4k.mp4',
    'request_id': 'req-123'
})
# Module 처리 후:
# video.processed → {path: '/data/output.mp4', size_gb: 2.5}

# 3. 진행 상황
controller.publish('dataset.train', {
    'model': 'resnet50',
    'epochs': 100
})
# Module이 주기적으로:
# train.progress → {epoch: 1, loss: 0.5}
# train.progress → {epoch: 2, loss: 0.45}
# ...
```

---

### 판단 기준표

| 상황                 | 권장           | 이유                   |
| -------------------- | -------------- | ---------------------- |
| f(10) = 55           | module.command | 작은 결과, 즉시 반환   |
| f(0, 10000) = [...]  | Bus            | 큰 배열, 스트리밍 가능 |
| User 조회 (< 100명)  | module.command | 즉시 반환 가능         |
| User 조회 (> 10만명) | Bus            | 배치 처리, 진행률      |
| 설정 읽기/쓰기       | module.command | 작은 JSON              |
| Video 처리           | Bus            | 대용량 파일 경로       |
| Dataset 학습         | Bus            | 진행 상황, 취소 가능   |
| Health check         | module.command | 즉시 응답              |

---

### bus.subscribe + bus.recv: 실시간 이벤트 수신

#### ✅ bus.subscribe + bus.recv 사용 (Long Polling)

**언제:**

- Module 결과를 실시간으로 받아야 할 때
- Data Layer 폴링 대체 (효율성 개선)
- 스트리밍 데이터 수신 (진행률, 부분 결과)
- 이벤트 기반 워크플로우

**예시:**

```python
# 1. 실시간 결과 수신 (폴링 대체)
# 이전: Data Layer 폴링
while True:
    result = daemon.data_get('fibonacci.result.req-123')
    if result:
        break
    time.sleep(0.1)

# 개선: bus.subscribe + bus.recv (Long Polling)
# Step 1: 구독 등록
sub_response = daemon.bus_subscribe('fibonacci.result')
subscriber_id = sub_response['result']['subscriber_id']

# Step 2: 이벤트 수신
event_response = daemon.bus_recv(subscriber_id, timeout=30000)
if not event_response['result'].get('timeout'):
    result = event_response['result']['data']

# 2. 스트리밍 진행률 수신
daemon.bus_publish('dataset.train', {'model': 'resnet50', 'epochs': 100})

# 구독 등록
sub = daemon.bus_subscribe('train.progress')
sub_id = sub['result']['subscriber_id']

for epoch in range(100):
    event = daemon.bus_recv(sub_id, timeout=60000)
    if not event['result'].get('timeout'):
        print(f"Epoch {event['result']['data']['epoch']}: Loss = {event['result']['data']['loss']}")

# 3. 배치 결과 수신
daemon.bus_publish('fibonacci.command', {'action': 'calculate_range', 'start': 0, 'count': 10000})

# 구독 등록
sub = daemon.bus_subscribe('fibonacci.stream')
sub_id = sub['result']['subscriber_id']

while True:
    event = daemon.bus_recv(sub_id, timeout=30000)
    if event['result'].get('timeout'):
        break  # 모든 결과 수신 완료
    process_batch(event['result']['data'])
```

**장점:**

- **효율적**: Data Layer 폴링보다 CPU 사용량 ~88% 감소
- **실시간**: 이벤트 도착 즉시 반환 (지연 최소화)
- **간단**: 1회 구독 + 반복 recv로 스트리밍 구현 가능

**주의사항:**

- Long Polling 방식: 1회 recv 호출당 1개 이벤트만 수신
- 여러 이벤트를 받으려면 bus.recv 반복 호출 필요
- timeout 설정 권장 (기본값: 30초)

---

## Best Practices

### 1. Module 독립성 유지

**❌ 잘못된 예:**

```python
# Module A
module.subscribe("module_b.result")  # 다른 Module 의존

# Module B
module.subscribe("module_a.command")  # 순환 의존
```

**✅ 올바른 예:**

```python
# Controller가 orchestration
controller.command('module_a', {'action': 'process'})
# 결과 확인
result_a = poll_data_layer('module_a.result')

# 다음 단계
controller.command('module_b', {
    'action': 'continue',
    'input': result_a
})
```

### 2. Topic Naming Convention

**권장 패턴:**

```
{module}.{category}.{action}

예:
- fibonacci.command       (Controller → Module 명령)
- fibonacci.result        (Module 결과 발행)
- fibonacci.progress      (진행 상황)
- video.processed         (처리 완료)
- system.shutdown         (시스템 이벤트)
```

**구독 제한 (자동 검증):**

Module은 다음 토픽만 구독 가능:
- ✅ `system.*` - 시스템 이벤트 (모든 Module)
- ✅ `{module_id}.*` - 자신의 토픽 (해당 Module만)
- ❌ `{other_module}.*` - 다른 Module 토픽 (거부)
- ❌ `*`, `#` - 전역 wildcard (거부)

**예시:**
```python
# Fibonacci Module

# ✅ 허용
module.subscribe("system.shutdown")      # 시스템 이벤트
module.subscribe("fibonacci.command")    # 자신의 명령
module.subscribe("fibonacci.*")          # 자신의 모든 이벤트
module.subscribe("fibonacci.#")          # 자신의 모든 이벤트 (recursive)

# ❌ 거부 (자동으로 에러 반환)
module.subscribe("calculator.result")   # 다른 Module
module.subscribe("logger.log")          # 다른 Module
module.subscribe("*")                    # 전역 wildcard
module.subscribe("#")                    # 전역 wildcard
```

**위반 시:**
```
ERROR: Module 'fibonacci' cannot subscribe to topic 'calculator.result'.
       Modules can only subscribe to 'system.*' or 'fibonacci.*' topics.
```

### 3. Request ID 관리

**2-level ID 사용:**

```python
# App → Controller
app_request_id = "app-123"

# Controller → Module (내부)
internal_id = f"req-{uuid.uuid4()}"

# Data Layer key
data_key = f"fibonacci.result.{internal_id}"

# 최종 응답
return {
    "id": app_request_id,  # App에게는 원래 ID
    "result": poll_data_layer(data_key)
}
```

### 4. Error Handling

**에러 코드 범위:**

```
0000-0999: Daemon 공통
1000-1999: Module A
2000-2999: Module B
3000-3999: Module C
5000-5999: Controller
```

### 5. Data Layer 키 관리

**명명 규칙:**

```
{module}.{type}.{identifier}

예:
- fibonacci.result.req-123
- video.processed.job-456
- config.settings
- cache.user.1001
```

**Eviction 고려:**

- 중요한 데이터: 파일로 저장 후 경로만 Data Layer에
- 임시 데이터: 인라인으로 저장
- SIEVE가 자동으로 오래된 항목 제거

---

## Examples

### Example 1: 동기 명령 (Small Data)

```python
# Controller
response = daemon.send_request('module.command', {
    'module': 'fibonacci',
    'id': 'cmd-1',
    'action': 'calculate',
    'n': 10
})

# Module이 Data Layer에 저장할 때까지 polling
result = poll_data_layer('fibonacci.result.cmd-1', timeout=5.0)

print(result)  # {"n": 10, "result": 55}
```

### Example 2: 비동기 이벤트 (Streaming)

```python
# Controller
daemon.send_request('bus.publish', {
    'topic': 'fibonacci.command',
    'data': {
        'action': 'calculate_range',
        'start': 0,
        'count': 10,
        'request_id': 'req-456'
    }
})

# Module (fibonacci)
def handle_event(msg):
    if msg['topic'] == 'fibonacci.command':
        data = msg['data']

        # 범위 계산하면서 스트리밍
        for i in range(data['start'], data['start'] + data['count']):
            value = fibonacci(i)

            # Bus에 발행 (스트리밍)
            publish('fibonacci.stream', {
                'request_id': data['request_id'],
                'index': i,
                'value': value
            })

        # 완료
        publish('fibonacci.done', {
            'request_id': data['request_id'],
            'count': data['count']
        })
```

### Example 3: 대용량 파일 처리

```python
# Controller
daemon.send_request('bus.publish', {
    'topic': 'video.process',
    'data': {
        'input_path': '/data/raw_video.mp4',
        'output_format': 'h264',
        'request_id': 'req-789'
    }
})

# Video Module
def handle_event(msg):
    if msg['topic'] == 'video.process':
        input_path = msg['data']['input_path']

        # 처리 (대용량)
        output_path = process_video(input_path)

        # 결과는 파일 경로만 Bus에 발행
        publish('video.processed', {
            'request_id': msg['data']['request_id'],
            'output_path': output_path,  # /data/processed.mp4
            'size_gb': 2.5
        })

        # Data Layer에도 저장 (Controller가 polling)
        data_write(f"video.result.{msg['data']['request_id']}", {
            'path': output_path
        })
```

### Example 4: Multi-step Workflow

```python
# Controller orchestration
def process_workflow():
    # Step 1: Module A 처리
    daemon.command('module_a', {'action': 'extract'})
    result_a = poll_data_layer('module_a.result')

    # Step 2: Module B 처리 (A의 결과 사용)
    daemon.command('module_b', {
        'action': 'transform',
        'input': result_a
    })
    result_b = poll_data_layer('module_b.result')

    # Step 3: Module C 처리 (B의 결과 사용)
    daemon.command('module_c', {
        'action': 'load',
        'data': result_b
    })

    return poll_data_layer('module_c.result')
```

---

## Summary

**Daemon V1의 핵심:**

1. **도구만 제공**: command, Bus, Data Layer
2. **정책은 개발자**: 언제 무엇을 쓸지 선택
3. **Module 독립성**: 서로 직접 통신 불가, Controller가 orchestration
4. **상황에 맞게**: 작은 데이터는 command, 큰 데이터/스트리밍은 Bus

**시작 가이드:**

- 간단한 기능 → module.command
- 복잡해지면 → Bus 고려
- 대용량이면 → Bus + 파일 경로
- 항상 Controller가 중심
