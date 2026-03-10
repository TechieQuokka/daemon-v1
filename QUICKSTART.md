# Quick Start Guide

## 빌드

```bash
# Windows
build.bat

# Linux/Mac
cargo build --release
```

## 실행

```bash
# 기본 설정으로 실행
./target/release/daemon_v1

# 설정 파일 지정
./target/release/daemon_v1 --config daemon-config.toml
```

## 테스트

```bash
# 단위 테스트
cargo test

# 통합 테스트
cargo test --test integration_test

# 모든 테스트
cargo test --all
```

## 예제 모듈 실행

### Rust 예제 (Echo Module)

```bash
# 빌드
cargo build --example echo_module

# 수동 실행 (테스트용)
./target/debug/examples/echo_module
```

### Python 예제

```bash
# 실행 권한 부여 (Linux/Mac)
chmod +x examples/echo_module.py

# 실행
python examples/echo_module.py
```

## Controller와 통신

Daemon이 실행 중일 때, TCP :9000으로 연결하여 JSON 명령을 보낼 수 있습니다.

### 예제: telnet으로 테스트

```bash
telnet localhost 9000
```

JSON 명령 입력:

```json
{"action":"daemon.status","params":null,"id":"req-1"}
```

응답:

```json
{"id":"req-1","success":true,"result":{"modules":0,"subscribers":0,"data_keys":0,"status":"running"}}
```

### 예제: Module 시작

```json
{"action":"module.start","params":{"name":"echo","path":"./target/debug/examples/echo_module","config":{}},"id":"req-2"}
```

### 예제: Data Layer에 데이터 저장

```json
{"action":"data.set","params":{"key":"count","value":123},"id":"req-3"}
```

### 예제: Data Layer에서 데이터 읽기

```json
{"action":"data.get","params":{"key":"count"},"id":"req-4"}
```

### 예제: Message Bus에 이벤트 발행

```json
{"action":"bus.publish","params":{"topic":"test.event","data":{"message":"hello"}},"id":"req-5"}
```

## 아키텍처 요약

```
┌─────────────┐
│  Controller │ ← TCP :9000 (JSON)
└──────┬──────┘
       │
┌──────▼──────────────────────┐
│      Daemon Core            │
│  ┌─────────┐  ┌──────────┐ │
│  │ Msg Bus │  │ Data     │ │
│  │ (FIFO)  │  │ (SIEVE)  │ │
│  └─────────┘  └──────────┘ │
└──────┬───────┬──────────────┘
       │       │
   ┌───▼──┐ ┌──▼──┐
   │Module│ │Module│ ← stdin/stdout JSON
   └──────┘ └─────┘
```

## 주요 기능

### 1. Message Bus
- **FIFO 보장**: 순서대로 전달
- **Wildcard 구독**: `user.*`, `calc.#`
- **Overflow 처리**: 큐 가득 차면 새 이벤트 거부

### 2. Data Layer
- **휘발성**: 메모리만 (재시작 시 소실)
- **SIEVE 알고리즘**: 자동 eviction
- **용량**: 기본 10,000 키

### 3. Module Protocol
- **stdin/stdout**: JSON 라인 기반
- **프로세스 격리**: 독립적 실행
- **다중 언어**: Rust, Python 등

### 4. Controller Protocol
- **TCP :9000**: JSON 라인 기반
- **비동기**: 여러 명령 동시 처리

## 에러 코드

- **0001**: Unknown command
- **0002**: Invalid format
- **0003**: Module not found
- **1001**: Calculator invalid input
- **1002**: Calculator overflow

## 다음 단계

1. 커스텀 모듈 개발 (`examples/` 참고)
2. Controller 클라이언트 라이브러리 작성
3. systemd 통합 (Linux)
4. Docker 배포

## 문제 해결

### Daemon이 시작 안 됨
- 포트 9000이 이미 사용 중인지 확인
- 로그 레벨을 DEBUG로 변경: `RUST_LOG=debug`

### Module이 시작 안 됨
- Module 실행 권한 확인
- Module 경로가 올바른지 확인
- stderr 출력 확인

### Message가 전달 안 됨
- 구독 패턴이 올바른지 확인 (`*` vs `#`)
- Bus 큐가 가득 찼는지 확인

## 더 알아보기

- [README.md](README.md) - 전체 문서
- [API Reference](API.md) - API 레퍼런스
- [Developer Guide](docs/dev-guide.md) - 개발자 가이드
- [Protocol Reference](docs/protocol-reference.md) - 프로토콜 레퍼런스
