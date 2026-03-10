# Daemon V1 API Reference

개발자가 Module 및 Controller를 개발할 때 사용하는 완전한 API 레퍼런스입니다.

## 목차

1. [Module API (stdin/stdout)](#module-api)
2. [Controller API (TCP)](#controller-api)
3. [Message Bus API](#message-bus-api)
4. [Data Layer API](#data-layer-api)
5. [Error Codes](#error-codes)
6. [개발 가이드](#development-guide)

---

# Module API

Module은 독립적인 프로세스로 실행되며, **stdin/stdout**을 통해 JSON 라인 기반으로 Daemon과 통신합니다.

## 통신 규칙

- **한 줄에 하나의 JSON 메시지**
- **개행 문자 (`\n`)로 구분**
- **UTF-8 인코딩**
- **stdin**: Daemon → Module (명령 수신)
- **stdout**: Module → Daemon (응답/요청 전송)
- **stderr**: 로그 출력 (Daemon이 캡처)

## Daemon → Module 메시지

### 1. `init` - 초기화

Module이 시작될 때 **첫 번째로 받는 메시지**.

```json
{
  "cmd": "init",
  "module_name": "calculator",
  "config": {
    "data_layer_path": "/data_layer",
    "custom_setting": "value"
  }
}
```

**필드:**
- `cmd`: `"init"` (고정)
- `module_name`: Module ID (string)
- `config`: 설정 객체 (자유 형식)

**Module 동작:**
1. 설정 파싱 및 초기화
2. 필요한 리소스 할당
3. 구독할 topic 등록 (`subscribe_request` 전송)
4. 준비 완료 로그 전송 (`log`)

**예제 응답:**
```json
{"type":"subscribe_request","topic":"calc.*"}
{"type":"log","message":"Calculator module initialized","level":"info"}
```

---

### 2. `command` - Controller 명령

Controller가 Module에 작업을 요청할 때 전송.

```json
{
  "cmd": "command",
  "id": "req-123",
  "action": "calculate",
  "n": 30
}
```

**필수 필드:**
- `cmd`: `"command"` (고정)
- `id`: 요청 ID (string) - 응답 시 동일한 ID 사용

**자유 형식:**
- `id` 외의 모든 필드는 Module이 정의
- 위 예시: `action`, `n`은 calculator 모듈 전용

**Module 동작:**
1. **즉시 ACK 전송** (명령 수신 확인)
2. 작업 수행
3. 결과를 Bus에 publish 또는 Data Layer에 저장
4. 실패 시 `error` 메시지 전송

**예제 응답:**
```json
{"type":"ack","id":"req-123"}
{"type":"publish","topic":"calc.done","metadata":{"result":832040,"elapsed_ms":0.009}}
```

**에러 예제:**
```json
{"type":"ack","id":"req-123"}
{"type":"error","id":"req-123","code":1002,"message":"integer overflow"}
```

---

### 3. `event` - Message Bus 이벤트

Module이 구독한 topic의 이벤트 수신.

```json
{
  "cmd": "event",
  "topic": "calc.done",
  "data": {
    "result": 832040,
    "elapsed_ms": 0.009
  },
  "publisher": "calculator",
  "timestamp": 1678901234
}
```

**필드:**
- `cmd`: `"event"` (고정)
- `topic`: 이벤트 topic (string)
- `data`: 이벤트 내용 (JSON object, optional)
- `publisher`: 발행한 Module ID (string)
- `timestamp`: Unix timestamp (u64)

**Module 동작:**
- 이벤트 처리 (로직 수행)
- 응답 불필요 (일방향 알림)

---

### 4. `shutdown` - 종료 요청

Daemon이 Module 종료를 요청.

```json
{
  "cmd": "shutdown",
  "force": false,
  "timeout": 5000
}
```

**필드:**
- `cmd`: `"shutdown"` (고정)
- `force`: 즉시 종료 여부 (bool, optional)
- `timeout`: 정리 시간 (ms, optional)

**Module 동작:**
- `force: false` → 리소스 정리 후 종료
- `force: true` → 즉시 종료
- timeout 내에 종료 완료

---

### 5. `data_response` - Data Layer 읽기 응답

Module이 `data_read` 요청 시 Daemon이 응답.

```json
{
  "cmd": "data_response",
  "key": "count",
  "value": 123
}
```

**Inline 데이터:**
```json
{
  "cmd": "data_response",
  "key": "count",
  "value": 123
}
```

**파일 참조:**
```json
{
  "cmd": "data_response",
  "key": "sales",
  "path": "/data_layer/sales_20260308.json"
}
```

**데이터 없음:**
```json
{
  "cmd": "data_response",
  "key": "unknown",
  "value": null
}
```

---

## Module → Daemon 메시지

### 1. `ack` - 명령 수신 확인

**필수**: 모든 `command`에 대해 즉시 전송.

```json
{
  "type": "ack",
  "id": "req-123"
}
```

**필드:**
- `type`: `"ack"` (고정)
- `id`: 수신한 command의 ID

---

### 2. `error` - 에러 응답

작업 실패 시 전송.

```json
{
  "type": "error",
  "id": "req-123",
  "code": 1002,
  "message": "integer overflow in fib(99999)"
}
```

**필드:**
- `type`: `"error"` (고정)
- `id`: 실패한 command의 ID
- `code`: 에러 코드 (u32) - [에러 코드 참고](#error-codes)
- `message`: 설명 (string, optional)

---

### 3. `publish` - Bus에 이벤트 발행

```json
{
  "type": "publish",
  "topic": "calc.done",
  "metadata": {
    "key": "fib_30",
    "result": 832040,
    "elapsed_ms": 0.009
  }
}
```

**필드:**
- `type`: `"publish"` (고정)
- `topic`: 이벤트 topic (string)
- `metadata`: 이벤트 내용 (JSON object)

**Topic 규칙:**
- 계층 구조: `module.category.action`
- 소문자 + 점(`.`) 구분자
- 예: `calculator.task.done`, `logger.level.error`

---

### 4. `subscribe_request` - Topic 구독

```json
{
  "type": "subscribe_request",
  "topic": "calc.*"
}
```

**필드:**
- `type`: `"subscribe_request"` (고정)
- `topic`: 구독 패턴 (string)

**Wildcard 패턴:**
- `*` : 정확히 1개 세그먼트
  - `"calc.*"` → `"calc.done"` ✓, `"calc.task.done"` ✗
- `#` : 0개 이상 세그먼트
  - `"calc.#"` → `"calc.done"` ✓, `"calc.task.done"` ✓
  - `"#"` → 모든 이벤트

---

### 5. `unsubscribe_request` - 구독 해제

```json
{
  "type": "unsubscribe_request",
  "topic": "calc.*"
}
```

---

### 6. `data_write` - Data Layer 저장

**Inline (작은 데이터):**
```json
{
  "type": "data_write",
  "key": "count",
  "value": 123
}
```

**파일 참조 (큰 데이터):**
```json
{
  "type": "data_write",
  "key": "sales",
  "path": "/data_layer/sales_20260308.json"
}
```

**필드:**
- `type`: `"data_write"` (고정)
- `key`: 키 (string)
- `value`: 값 (JSON, inline 방식)
- `path`: 파일 경로 (string, 파일 방식)

**규칙:**
- `value` 또는 `path` 중 **하나만** 사용
- 파일은 Module이 직접 `/data_layer/`에 생성
- Daemon은 경로만 기록

---

### 7. `data_read` - Data Layer 읽기

```json
{
  "type": "data_read",
  "key": "count"
}
```

**응답:** Daemon이 `data_response` 전송

---

### 8. `data_delete` - Data Layer 삭제

```json
{
  "type": "data_delete",
  "key": "count"
}
```

---

### 9. `log` - 로그 메시지

```json
{
  "type": "log",
  "message": "Calculation completed",
  "level": "info"
}
```

**필드:**
- `type`: `"log"` (고정)
- `message`: 로그 내용 (string)
- `level`: 로그 레벨 (optional)
  - `"error"`, `"warn"`, `"info"` (기본), `"debug"`, `"trace"`

---

## Module 개발 예제 (Python)

```python
#!/usr/bin/env python3
import sys
import json

def send(msg):
    print(json.dumps(msg), flush=True)

def main():
    for line in sys.stdin:
        msg = json.loads(line)
        cmd = msg.get('cmd')

        if cmd == 'init':
            # 초기화
            send({'type': 'subscribe_request', 'topic': 'my.*'})
            send({'type': 'log', 'message': 'Module ready'})

        elif cmd == 'command':
            # 명령 처리
            cmd_id = msg['id']
            send({'type': 'ack', 'id': cmd_id})

            # 작업 수행
            result = process(msg)

            # 결과 발행
            send({
                'type': 'publish',
                'topic': 'my.done',
                'metadata': {'result': result}
            })

        elif cmd == 'event':
            # 이벤트 처리
            handle_event(msg)

        elif cmd == 'shutdown':
            cleanup()
            break

if __name__ == '__main__':
    main()
```

---

# Controller API

Controller는 TCP :9000으로 Daemon에 연결하여 JSON 라인 기반으로 통신합니다.

## 연결 방법

```python
import socket
import json

sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.connect(('127.0.0.1', 9000))

def send_request(action, params, req_id):
    request = {
        'action': action,
        'params': params,
        'id': req_id
    }
    sock.sendall((json.dumps(request) + '\n').encode())

    response = sock.recv(4096).decode().strip()
    return json.loads(response)
```

## Request 형식

```json
{
  "action": "module.start",
  "params": {
    "name": "calculator",
    "path": "./modules/calculator",
    "config": {}
  },
  "id": "req-123"
}
```

**필수 필드:**
- `action`: 명령 종류 (string)
- `params`: 파라미터 (JSON object, optional)
- `id`: 요청 ID (string) - 응답 매칭용

## Response 형식

**성공:**
```json
{
  "id": "req-123",
  "success": true,
  "result": {
    "module_id": "calc-abc123"
  }
}
```

**실패:**
```json
{
  "id": "req-123",
  "success": false,
  "error": "Module not found"
}
```

---

## Available Actions

### 1. Module 관리

#### `module.start` - Module 시작

```json
{
  "action": "module.start",
  "params": {
    "name": "calculator",
    "path": "./modules/calculator",
    "config": {
      "data_layer_path": "/data_layer"
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
    "module_id": "calculator"
  }
}
```

---

#### `module.stop` - Module 종료

```json
{
  "action": "module.stop",
  "params": {
    "id": "calculator",
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

#### `module.list` - Module 목록

```json
{
  "action": "module.list",
  "params": null,
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
        "id": "calculator",
        "path": "./modules/calculator",
        "status": "running",
        "pid": 12345,
        "subscriptions": ["calc.*"]
      }
    ]
  }
}
```

---

#### `health_check` - Module 상태 확인

```json
{
  "action": "health_check",
  "params": {
    "module": "calculator"
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
    "module_id": "calculator",
    "status": "running",
    "pid": 12345
  }
}
```

---

### 2. Data Layer 접근

#### `data.get` - 데이터 읽기

```json
{
  "action": "data.get",
  "params": {
    "key": "count"
  },
  "id": "req-5"
}
```

**Response (inline):**
```json
{
  "id": "req-5",
  "success": true,
  "result": {
    "key": "count",
    "value": 123
  }
}
```

**Response (file):**
```json
{
  "id": "req-5",
  "success": true,
  "result": {
    "key": "sales",
    "path": "/data_layer/sales_20260308.json"
  }
}
```

---

#### `data.set` - 데이터 저장

**Inline:**
```json
{
  "action": "data.set",
  "params": {
    "key": "count",
    "value": 123
  },
  "id": "req-6"
}
```

**File:**
```json
{
  "action": "data.set",
  "params": {
    "key": "sales",
    "path": "/data_layer/sales_20260308.json"
  },
  "id": "req-7"
}
```

**Response:**
```json
{
  "id": "req-6",
  "success": true,
  "result": {
    "key": "count",
    "status": "set"
  }
}
```

---

#### `data.delete` - 데이터 삭제

```json
{
  "action": "data.delete",
  "params": {
    "key": "count"
  },
  "id": "req-8"
}
```

**Response:**
```json
{
  "id": "req-8",
  "success": true,
  "result": {
    "key": "count",
    "deleted": true
  }
}
```

---

#### `data.list` - 모든 키 조회

```json
{
  "action": "data.list",
  "params": null,
  "id": "req-9"
}
```

**Response:**
```json
{
  "id": "req-9",
  "success": true,
  "result": {
    "keys": ["count", "config", "sales"]
  }
}
```

---

### 3. Message Bus

#### `bus.publish` - 이벤트 발행

```json
{
  "action": "bus.publish",
  "params": {
    "topic": "system.alert",
    "data": {
      "level": "warning",
      "message": "High memory usage"
    }
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
    "status": "published"
  }
}
```

---

#### `bus.subscribe` - 이벤트 구독 (Long Polling)

Bus 이벤트를 실시간으로 수신합니다.

```json
{
  "action": "bus.subscribe",
  "params": {
    "topic": "fibonacci.result",
    "timeout": 30000
  },
  "id": "req-11"
}
```

**Parameters:**
- `topic` (string, required): 구독할 토픽 패턴 (wildcards 지원: `*`, `#`)
- `timeout` (number, optional): 타임아웃 (밀리초, 기본값: 30000)

**Response (이벤트 수신):**
```json
{
  "id": "req-11",
  "success": true,
  "result": {
    "topic": "fibonacci.result",
    "data": {
      "n": 10,
      "result": 55
    },
    "publisher": "fibonacci",
    "timestamp": 1234567890
  }
}
```

**Response (타임아웃):**
```json
{
  "id": "req-11",
  "success": true,
  "result": {
    "topic": "fibonacci.result",
    "data": null,
    "timeout": true
  }
}
```

**동작 방식:**
1. Controller가 subscribe 요청을 보내면 Daemon이 이벤트 대기
2. 이벤트 발생 시 즉시 응답 반환 (Fast Path)
3. timeout 초과 시 `timeout: true` 반환
4. 응답 후 자동으로 구독 해제 (1회성)

**활용:**
- Data Layer 폴링 대체 (효율성 ~88% 향상)
- 실시간 결과 수신
- 스트리밍 데이터 처리

**주의:**
- Long Polling 방식 (1 request = 1 event)
- 여러 이벤트를 받으려면 반복 요청 필요
- Module은 bus.subscribe 불가 (Controller만 가능)

---

### 4. Daemon 관리

#### `daemon.status` - Daemon 상태

```json
{
  "action": "daemon.status",
  "params": null,
  "id": "req-11"
}
```

**Response:**
```json
{
  "id": "req-11",
  "success": true,
  "result": {
    "modules": 3,
    "subscribers": 5,
    "data_keys": 10,
    "status": "running"
  }
}
```

---

#### `daemon.shutdown` - Daemon 종료

```json
{
  "action": "daemon.shutdown",
  "params": null,
  "id": "req-12"
}
```

---

## Controller 개발 예제 (Python)

```python
import socket
import json

class DaemonClient:
    def __init__(self, host='127.0.0.1', port=9000):
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.connect((host, port))
        self._req_id = 0

    def _send(self, action, params=None):
        self._req_id += 1
        request = {
            'action': action,
            'params': params,
            'id': f'req-{self._req_id}'
        }
        self.sock.sendall((json.dumps(request) + '\n').encode())

        response = self.sock.recv(4096).decode().strip()
        return json.loads(response)

    def start_module(self, name, path, config=None):
        return self._send('module.start', {
            'name': name,
            'path': path,
            'config': config or {}
        })

    def get_data(self, key):
        return self._send('data.get', {'key': key})

    def publish(self, topic, data):
        return self._send('bus.publish', {
            'topic': topic,
            'data': data
        })

    def subscribe(self, topic, timeout=30000):
        return self._send('bus.subscribe', {
            'topic': topic,
            'timeout': timeout
        })

# 사용 예시
client = DaemonClient()
result = client.start_module('calc', './modules/calculator')
print(result)

# 실시간 이벤트 수신 (Long Polling)
event = client.subscribe('fibonacci.result', timeout=30000)
if not event['result'].get('timeout'):
    print(f"Received: {event['result']['data']}")
```

---

# Message Bus API

## Topic 형식

```
module.category.action
```

**예시:**
- `calculator.task.done`
- `logger.level.error`
- `monitor.system.tick`

**규칙:**
- 소문자만 사용
- 점(`.`)으로 계층 구분
- 명확하고 의미 있는 이름

---

## Wildcard 패턴

### `*` - Single Wildcard (정확히 1개 세그먼트)

```
"user.*"       → "user.created" ✓
               → "user.updated" ✓
               → "user.profile.changed" ✗

"*.created"    → "user.created" ✓
               → "post.created" ✓
               → "user.profile.created" ✗
```

### `#` - Multi Wildcard (0개 이상 세그먼트)

```
"user.#"       → "user.created" ✓
               → "user.profile.updated" ✓
               → "user.profile.avatar.changed" ✓

"#.created"    → "created" ✓
               → "user.created" ✓
               → "user.profile.created" ✓

"#"            → 모든 이벤트 ✓
```

### 혼합 패턴

```
"user.*.#"     → "user.profile.updated" ✓
               → "user.profile.avatar.changed" ✓
               → "user.created" ✓ (# matches 0 segments)
               → "user" ✗ (* requires 1 segment)
```

---

## 순서 보장

- **FIFO**: 발행 순서대로 전달
- **동일 Module**: A가 먼저 publish → A가 먼저 받음
- **다른 Module**: 발행 시간 순서 유지

---

## Overflow 처리

Bus 큐가 가득 차면 (기본: 10,000개):
- **새 이벤트 거부**
- **FIFO 순서 유지**
- **Module에 에러 응답**

---

# Data Layer API

## 특성

- **휘발성**: 메모리만 (재시작 시 소실)
- **용량**: 기본 10,000 키
- **Eviction**: SIEVE 알고리즘 (자동)
- **동시성**: Thread-safe (RwLock)

---

## 저장 방식

### Inline (작은 데이터, < 1MB 권장)

```json
{
  "type": "data_write",
  "key": "config",
  "value": {
    "host": "localhost",
    "port": 8000
  }
}
```

### File Reference (큰 데이터)

1. Module이 파일 생성:
   ```bash
   /data_layer/sales_20260308.json
   ```

2. 경로만 저장:
   ```json
   {
     "type": "data_write",
     "key": "sales",
     "path": "/data_layer/sales_20260308.json"
   }
   ```

---

## SIEVE Eviction

용량 초과 시 자동 삭제:
1. SIEVE 알고리즘이 "덜 중요한" 키 선택
2. 자동 삭제
3. 새 키 저장
4. **Module 개입 불필요**

**특징:**
- LRU보다 효율적 (NSDI'24 논문)
- 접근 패턴 고려
- 예측 가능한 동작

---

# Error Codes

에러 코드는 모듈별로 범위 할당:

## Daemon 공통 (0000-0999)

| Code | Name | Description |
|------|------|-------------|
| 1 | UNKNOWN_COMMAND | 알 수 없는 명령 |
| 2 | INVALID_FORMAT | 잘못된 JSON 형식 |
| 3 | MODULE_NOT_FOUND | Module을 찾을 수 없음 |

## Calculator Module (1000-1999)

| Code | Name | Description |
|------|------|-------------|
| 1001 | INVALID_INPUT | 잘못된 입력 |
| 1002 | OVERFLOW | 정수 오버플로우 |
| 1003 | TIMEOUT | 계산 시간 초과 |

## Logger Module (2000-2999)

| Code | Name | Description |
|------|------|-------------|
| 2001 | FILE_NOT_FOUND | 로그 파일 없음 |
| 2002 | PERMISSION_DENIED | 권한 부족 |

## 사용자 정의 Module

- **3000-3999**: Monitor
- **4000-4999**: ...
- Module마다 1000개씩 할당

---

# Development Guide

## Module 개발 체크리스트

### 1. 초기화 (`init`)
- [ ] 설정 파싱
- [ ] 리소스 할당
- [ ] Topic 구독 (`subscribe_request`)
- [ ] 준비 완료 로그

### 2. 명령 처리 (`command`)
- [ ] **즉시 ACK 전송** (필수!)
- [ ] 작업 수행
- [ ] 결과 publish 또는 저장
- [ ] 에러 시 `error` 전송

### 3. 이벤트 처리 (`event`)
- [ ] 관심 이벤트만 처리
- [ ] 응답 불필요

### 4. 종료 (`shutdown`)
- [ ] 리소스 정리
- [ ] 진행 중 작업 완료/취소
- [ ] 정상 종료

### 5. 에러 처리
- [ ] 모든 에러에 적절한 코드 할당
- [ ] 명확한 에러 메시지
- [ ] 복구 가능한 에러는 재시도

---

## Controller 개발 체크리스트

### 1. 연결 관리
- [ ] TCP 연결 유지
- [ ] 재연결 로직
- [ ] 타임아웃 처리

### 2. Request/Response 매칭
- [ ] 고유한 `id` 생성
- [ ] 응답 대기 (비동기)
- [ ] 타임아웃 처리

### 3. 에러 처리
- [ ] `success: false` 확인
- [ ] `error` 메시지 파싱
- [ ] 재시도 로직

---

## 디버깅 팁

### Module 디버깅

```python
# stderr로 디버그 로그 출력 (stdout은 프로토콜용)
import sys
sys.stderr.write(f"DEBUG: {msg}\n")
sys.stderr.flush()
```

### 메시지 검증

```bash
# Module 수동 테스트
echo '{"cmd":"init","module_name":"test","config":{}}' | python my_module.py
```

### Daemon 로그

```bash
# DEBUG 모드로 실행
RUST_LOG=debug ./target/debug/daemon_v1
```

---

## 성능 최적화

### Module
- ACK는 즉시 전송 (작업 전)
- 무거운 작업은 비동기 처리
- 큰 데이터는 파일 참조 사용

### Controller
- 연결 재사용 (connection pooling)
- 비동기 요청 (여러 명령 동시 전송)
- 타임아웃 적절히 설정

---

## 보안 고려사항

### Module
- stdin 입력 검증
- 파일 경로 검증 (path traversal 방지)
- 리소스 제한 (메모리, CPU)

### Controller
- 인증 토큰 (향후 지원)
- TLS 암호화 (향후 지원)
- Rate limiting

---

## 예제 시나리오

### 시나리오 1: 간단한 계산 Module

```python
# calculator.py
while True:
    msg = json.loads(input())
    if msg['cmd'] == 'command':
        send({'type': 'ack', 'id': msg['id']})
        result = fibonacci(msg['n'])
        send({
            'type': 'publish',
            'topic': 'calc.done',
            'metadata': {'result': result}
        })
```

### 시나리오 2: Module 간 협업

```
1. Module A → Bus: publish("data.ready", {...})
2. Module B (구독 중) → event 수신
3. Module B → Data Layer: data_read("processed")
4. Module B → Bus: publish("data.processed", {...})
```

### 시나리오 3: Controller 워크플로우

```python
# 1. Module 시작
client.start_module('processor', './modules/processor')

# 2. 데이터 저장
client.set_data('input', {'values': [1, 2, 3]})

# 3. 작업 트리거 (Bus 이벤트)
client.publish('task.start', {'input_key': 'input'})

# 4. 결과 대기 (polling 또는 Bus 구독)
result = client.get_data('output')
```

---

## 추가 리소스

- [README.md](README.md) - 전체 아키텍처
- [QUICKSTART.md](QUICKSTART.md) - 빠른 시작
- [examples/](examples/) - 예제 코드
- [tests/](tests/) - 통합 테스트

---

**버전**: 0.1.0
**최종 수정**: 2026-03-08
