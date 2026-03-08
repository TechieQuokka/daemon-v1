# Daemon V1 Tutorial

실전 예제를 통해 Daemon V1 사용법을 배웁니다.

## 목차

1. [시작하기](#시작하기)
2. [첫 번째 Module 만들기](#첫-번째-module-만들기)
3. [Controller로 제어하기](#controller로-제어하기)
4. [Module 간 통신](#module-간-통신)
5. [실전 시나리오](#실전-시나리오)

---

## 시작하기

### 1. Daemon 실행

```bash
# 빌드
cargo build --release

# 실행
./target/release/daemon_v1
```

출력:
```
✓ Message bus initialized
✓ Data layer initialized (capacity: 10000)
✓ Module registry initialized
✓ IPC server listening on 127.0.0.1:9000
✓ Daemon V1 is running

Press Ctrl+C to shutdown
```

### 2. 상태 확인

새 터미널에서:
```bash
# telnet으로 연결
telnet localhost 9000

# 명령 입력
{"action":"daemon.status","params":null,"id":"req-1"}

# 응답
{"id":"req-1","success":true,"result":{"modules":0,"subscribers":0,"data_keys":0,"status":"running"}}
```

---

## 첫 번째 Module 만들기

### Step 1: 간단한 Echo Module (Python)

`my_echo.py`:
```python
#!/usr/bin/env python3
import sys
import json

def send(msg):
    """Daemon에게 메시지 전송"""
    print(json.dumps(msg), flush=True)

def log(message):
    """로그 출력"""
    send({'type': 'log', 'message': message, 'level': 'info'})

# 초기화
for line in sys.stdin:
    msg = json.loads(line.strip())
    cmd = msg.get('cmd')

    if cmd == 'init':
        # 초기화
        module_name = msg['module_name']
        log(f"Echo module '{module_name}' starting...")

        # 구독
        send({'type': 'subscribe_request', 'topic': 'echo.*'})
        log("Subscribed to 'echo.*'")

    elif cmd == 'command':
        # 명령 처리
        cmd_id = msg['id']

        # ACK 전송 (필수!)
        send({'type': 'ack', 'id': cmd_id})

        # Echo back
        send({
            'type': 'publish',
            'topic': 'echo.response',
            'metadata': {
                'original': msg,
                'echoed_at': __import__('time').time()
            }
        })

    elif cmd == 'event':
        # 이벤트 수신
        topic = msg['topic']
        data = msg.get('data', {})
        log(f"Received event: {topic}")

        # Echo back
        send({
            'type': 'publish',
            'topic': topic.replace('echo.', 'echo.response.'),
            'metadata': data
        })

    elif cmd == 'shutdown':
        log("Shutting down...")
        break

log("Echo module stopped")
```

### Step 2: 실행 권한 부여

```bash
chmod +x my_echo.py
```

### Step 3: Controller로 시작

Python Controller:
```python
import socket
import json

sock = socket.socket()
sock.connect(('127.0.0.1', 9000))

def send_command(action, params):
    request = {
        'action': action,
        'params': params,
        'id': 'req-1'
    }
    sock.sendall((json.dumps(request) + '\n').encode())
    response = sock.recv(4096).decode()
    print(response)

# Module 시작
send_command('module.start', {
    'name': 'echo',
    'path': './my_echo.py',
    'config': {}
})
```

---

## Controller로 제어하기

### Python Controller 클래스

```python
class DaemonClient:
    def __init__(self, host='127.0.0.1', port=9000):
        self.sock = socket.socket()
        self.sock.connect((host, port))
        self._id = 0

    def _request(self, action, params=None):
        self._id += 1
        req = {
            'action': action,
            'params': params,
            'id': f'req-{self._id}'
        }
        self.sock.sendall((json.dumps(req) + '\n').encode())
        resp = json.loads(self.sock.recv(4096).decode())
        return resp

    def start_module(self, name, path, config=None):
        return self._request('module.start', {
            'name': name,
            'path': path,
            'config': config or {}
        })

    def list_modules(self):
        return self._request('module.list')

    def publish(self, topic, data):
        return self._request('bus.publish', {
            'topic': topic,
            'data': data
        })

    def get_data(self, key):
        return self._request('data.get', {'key': key})

    def set_data(self, key, value):
        return self._request('data.set', {
            'key': key,
            'value': value
        })
```

### 사용 예시

```python
client = DaemonClient()

# 1. Module 시작
result = client.start_module('echo', './my_echo.py')
print(result)
# {"id":"req-1","success":true,"result":{"module_id":"echo"}}

# 2. Module 목록
modules = client.list_modules()
print(modules)
# {"id":"req-2","success":true,"result":{"modules":[...]}}

# 3. Data 저장
client.set_data('counter', 42)

# 4. Data 읽기
result = client.get_data('counter')
print(result)
# {"id":"req-4","success":true,"result":{"key":"counter","value":42}}

# 5. 이벤트 발행
client.publish('echo.test', {'message': 'Hello!'})
```

---

## Module 간 통신

### 시나리오: Producer → Consumer

#### Producer Module (`producer.py`)

```python
import sys
import json
import time

def send(msg):
    print(json.dumps(msg), flush=True)

for line in sys.stdin:
    msg = json.loads(line)

    if msg['cmd'] == 'init':
        send({'type': 'log', 'message': 'Producer started'})

    elif msg['cmd'] == 'command':
        # ACK
        send({'type': 'ack', 'id': msg['id']})

        # 데이터 생성
        data = {
            'value': 42,
            'timestamp': time.time()
        }

        # Data Layer에 저장
        send({
            'type': 'data_write',
            'key': 'produced_data',
            'value': data
        })

        # Bus에 알림
        send({
            'type': 'publish',
            'topic': 'data.ready',
            'metadata': {'key': 'produced_data'}
        })

    elif msg['cmd'] == 'shutdown':
        break
```

#### Consumer Module (`consumer.py`)

```python
import sys
import json

def send(msg):
    print(json.dumps(msg), flush=True)

for line in sys.stdin:
    msg = json.loads(line)

    if msg['cmd'] == 'init':
        # 'data.ready' 구독
        send({'type': 'subscribe_request', 'topic': 'data.ready'})
        send({'type': 'log', 'message': 'Consumer ready'})

    elif msg['cmd'] == 'event':
        # 이벤트 수신
        if msg['topic'] == 'data.ready':
            key = msg['data']['key']

            # Data Layer에서 읽기
            send({'type': 'data_read', 'key': key})

    elif msg['cmd'] == 'data_response':
        # 데이터 수신
        value = msg['value']
        send({'type': 'log', 'message': f'Consumed: {value}'})

        # 처리 완료 알림
        send({
            'type': 'publish',
            'topic': 'data.processed',
            'metadata': {'status': 'done'}
        })

    elif msg['cmd'] == 'shutdown':
        break
```

#### Controller 워크플로우

```python
client = DaemonClient()

# 1. Module 시작
client.start_module('producer', './producer.py')
client.start_module('consumer', './consumer.py')

# 2. Producer 트리거
# (Module에게 명령을 전송하는 기능은 TODO)

# 3. 결과 확인
result = client.get_data('produced_data')
print(result)
```

---

## 실전 시나리오

### 시나리오 1: 로그 수집 시스템

#### 구조
```
Application Module → log.* 이벤트 발행
                     ↓
                  Log Collector Module (구독: log.*)
                     ↓
                  파일/DB 저장
```

#### Log Collector Module

```python
import sys
import json
from datetime import datetime

LOG_FILE = '/var/log/daemon_logs.txt'

def send(msg):
    print(json.dumps(msg), flush=True)

def save_log(topic, data):
    timestamp = datetime.now().isoformat()
    with open(LOG_FILE, 'a') as f:
        f.write(f"[{timestamp}] {topic}: {json.dumps(data)}\n")

for line in sys.stdin:
    msg = json.loads(line)

    if msg['cmd'] == 'init':
        # 모든 log.* 이벤트 구독
        send({'type': 'subscribe_request', 'topic': 'log.#'})
        send({'type': 'log', 'message': 'Log collector started'})

    elif msg['cmd'] == 'event':
        # log.* 이벤트 저장
        if msg['topic'].startswith('log.'):
            save_log(msg['topic'], msg.get('data', {}))

    elif msg['cmd'] == 'shutdown':
        break
```

---

### 시나리오 2: 작업 큐 시스템

#### 구조
```
Controller → task.enqueue 발행
            ↓
         Worker Module (구독: task.enqueue)
            ↓
         작업 처리 → task.completed 발행
```

#### Worker Module

```python
import sys
import json
import time

def send(msg):
    print(json.dumps(msg), flush=True)

def process_task(task_data):
    # 작업 처리 시뮬레이션
    time.sleep(1)
    return {'result': f"Processed {task_data}"}

for line in sys.stdin:
    msg = json.loads(line)

    if msg['cmd'] == 'init':
        send({'type': 'subscribe_request', 'topic': 'task.enqueue'})
        send({'type': 'log', 'message': 'Worker ready'})

    elif msg['cmd'] == 'event':
        if msg['topic'] == 'task.enqueue':
            task_id = msg['data']['task_id']
            task_data = msg['data']['data']

            send({'type': 'log', 'message': f'Processing task {task_id}'})

            # 작업 처리
            result = process_task(task_data)

            # 결과 저장
            send({
                'type': 'data_write',
                'key': f'task_result_{task_id}',
                'value': result
            })

            # 완료 알림
            send({
                'type': 'publish',
                'topic': 'task.completed',
                'metadata': {
                    'task_id': task_id,
                    'result_key': f'task_result_{task_id}'
                }
            })

    elif msg['cmd'] == 'shutdown':
        break
```

#### Controller

```python
client = DaemonClient()

# Worker 시작
client.start_module('worker', './worker.py')

# 작업 등록
for i in range(5):
    client.publish('task.enqueue', {
        'task_id': i,
        'data': f'task-{i}'
    })
    print(f'Task {i} enqueued')

# 결과 확인
time.sleep(6)  # 작업 완료 대기

for i in range(5):
    result = client.get_data(f'task_result_{i}')
    print(f'Task {i} result:', result)
```

---

### 시나리오 3: 실시간 모니터링

#### Monitor Module

```python
import sys
import json
import psutil
import time

def send(msg):
    print(json.dumps(msg), flush=True)

def collect_metrics():
    return {
        'cpu_percent': psutil.cpu_percent(interval=1),
        'memory_percent': psutil.virtual_memory().percent,
        'disk_percent': psutil.disk_usage('/').percent
    }

for line in sys.stdin:
    msg = json.loads(line)

    if msg['cmd'] == 'init':
        send({'type': 'log', 'message': 'Monitor started'})

    elif msg['cmd'] == 'command':
        send({'type': 'ack', 'id': msg['id']})

        # 메트릭 수집
        metrics = collect_metrics()

        # 저장
        send({
            'type': 'data_write',
            'key': 'current_metrics',
            'value': metrics
        })

        # 알림
        send({
            'type': 'publish',
            'topic': 'monitor.metrics',
            'metadata': metrics
        })

        # 임계값 확인
        if metrics['cpu_percent'] > 80:
            send({
                'type': 'publish',
                'topic': 'monitor.alert',
                'metadata': {
                    'type': 'cpu_high',
                    'value': metrics['cpu_percent']
                }
            })

    elif msg['cmd'] == 'shutdown':
        break
```

---

## 디버깅 팁

### 1. Module 로그 확인

Module의 stderr는 Daemon이 캡처합니다:

```python
# Module 코드
import sys
sys.stderr.write("DEBUG: Processing started\n")
sys.stderr.flush()
```

Daemon 로그에서 확인:
```bash
RUST_LOG=debug cargo run
```

### 2. 메시지 검증

수동으로 Module 테스트:
```bash
echo '{"cmd":"init","module_name":"test","config":{}}' | python my_module.py
```

### 3. Protocol 검증

잘못된 JSON:
```python
# ✗ 잘못된 예
print("not json")  # 파싱 에러!

# ✓ 올바른 예
print(json.dumps({'type': 'log', 'message': 'ok'}))
```

---

## 다음 단계

1. **API 문서 읽기**: [API.md](API.md)
2. **예제 코드 실행**: `examples/`
3. **프로젝트 구조 이해**: [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md)
4. **실전 프로젝트 개발**

---

## 자주 묻는 질문

### Q: Module이 시작되지 않습니다
**A**:
- 실행 권한 확인: `chmod +x my_module.py`
- 경로 확인: 절대 경로 사용
- Daemon 로그 확인: `RUST_LOG=debug`

### Q: 이벤트가 수신되지 않습니다
**A**:
- 구독 패턴 확인: `*` vs `#`
- `subscribe_request` 전송 확인
- topic 철자 확인

### Q: Data Layer 데이터가 사라집니다
**A**:
- **정상**: Data Layer는 휘발성 (메모리만)
- 영속성 필요시 Module이 파일/DB에 저장

### Q: Module 간 직접 통신은?
**A**:
- **불가능**: Module은 독립 프로세스
- **대안**: Message Bus 또는 Data Layer 사용

---

**행운을 빕니다!** 🚀
