# Project Structure

```
DaemonV1/
├── Cargo.toml                      # 프로젝트 설정 및 의존성
├── build.bat                       # Windows 빌드 스크립트
├── .gitignore                      # Git 제외 파일
│
├── README.md                       # 전체 문서
├── QUICKSTART.md                   # 빠른 시작 가이드
├── PROJECT_STRUCTURE.md            # 이 파일
│
├── config.example.toml             # 설정 파일 예시
│
├── src/
│   ├── main.rs                     # 엔트리 포인트
│   ├── lib.rs                      # 라이브러리 루트
│   ├── error.rs                    # 에러 타입 정의
│   │
│   ├── protocol/                   # 프로토콜 정의
│   │   ├── mod.rs
│   │   ├── module.rs               # Module ↔ Daemon 메시지
│   │   ├── controller.rs           # Controller ↔ Daemon 메시지
│   │   └── codec.rs                # JSON 라인 코덱
│   │
│   ├── bus/                        # Message Bus
│   │   ├── mod.rs
│   │   ├── types.rs                # 메시지, 설정 타입
│   │   ├── router.rs               # Topic 패턴 매칭 (wildcard)
│   │   ├── subscriber.rs           # 구독 레지스트리
│   │   └── processor.rs            # FIFO 메시지 프로세서
│   │
│   ├── storage/                    # Data Layer
│   │   ├── mod.rs
│   │   ├── types.rs                # 설정, DataEntry 타입
│   │   ├── sieve.rs                # SIEVE eviction 알고리즘
│   │   └── cache.rs                # Thread-safe wrapper
│   │
│   ├── module/                     # Module 관리
│   │   ├── mod.rs
│   │   ├── manager.rs              # Module 메시지 처리 및 조율
│   │   ├── process.rs              # 프로세스 spawn, stdio 통신
│   │   └── registry.rs             # Module 레지스트리
│   │
│   ├── controller/                 # IPC Server
│   │   ├── mod.rs
│   │   ├── server.rs               # TCP 리스너
│   │   └── handler.rs              # 명령 처리
│   │
│   └── config/                     # 설정 관리
│       ├── mod.rs
│       └── daemon.rs               # DaemonConfig
│
├── examples/
│   ├── echo_module.rs              # Rust 예제 모듈
│   └── echo_module.py              # Python 예제 모듈
│
└── tests/
    └── integration_test.rs         # 통합 테스트
```

## 핵심 컴포넌트

### 1. Protocol Layer (src/protocol/)
- **module.rs**: `DaemonToModule`, `ModuleToDaemon` 메시지 정의
- **controller.rs**: `ControllerRequest`, `ControllerResponse` 정의
- **codec.rs**: JSON 라인 기반 인코딩/디코딩

### 2. Message Bus (src/bus/)
- **processor.rs**: FIFO 보장하는 순차 처리
- **router.rs**: Wildcard 패턴 매칭 (`*`, `#`)
- **subscriber.rs**: 구독 관리 (broadcast, 다중 구독)
- **types.rs**: `BusMessage`, `MessageSource`

### 3. Data Layer (src/storage/)
- **sieve.rs**: SIEVE 캐시 eviction 알고리즘 (NSDI'24)
- **cache.rs**: RwLock 기반 thread-safe wrapper
- **types.rs**: `DataEntry` (Inline | File)

### 4. Module Management (src/module/)
- **manager.rs**: `ModuleManager` - 모듈 메시지 처리, Bus/DataLayer 통합
- **process.rs**: `ModuleProcess` - 프로세스 spawn, stdin/stdout 통신
- **registry.rs**: `ModuleRegistry` - 활성 모듈 관리, 상태 추적

### 5. IPC Server (src/controller/)
- **server.rs**: `IpcServer` - TCP :9000 리스너
- **handler.rs**: `CommandHandler` - 명령 라우팅 및 처리

### 6. Configuration (src/config/)
- **daemon.rs**: `DaemonConfig` - TOML 설정 로드/저장

## 데이터 흐름

### Controller → Module 명령 흐름

```
Controller (TCP)
  ↓ JSON request
IpcServer (server.rs)
  ↓ parse
CommandHandler (handler.rs)
  ↓ module.start action
ModuleManager (manager.rs)
  ↓ spawn process + message handler
ModuleProcess (process.rs)
  ↓ stdin JSON
Module (외부 프로세스)
```

### Module → Bus → Module 이벤트 흐름

```
Module A (stdout)
  ↓ JSON: {type: "publish"}
ModuleProcess
  ↓ from_module_rx
ModuleManager.handle_module_message()
  ↓ ModuleToDaemon::Publish
MessageBus.publish()
  ↓ mpsc channel
Sequential Processor (processor.rs)
  ↓ route
SubscriptionRegistry (subscriber.rs)
  ↓ pattern matching
Module B's receiver
  ↓ forwarding task
  ↓ DaemonToModule::Event
Module B (stdin)
```

### Data Layer 접근 흐름

```
Module
  ↓ {type: "data_write"}
ModuleProcess
  ↓ from_module_rx
ModuleManager.handle_module_message()
  ↓ ModuleToDaemon::DataWrite
DataLayer.set()
  ↓ RwLock write
SieveCache
  ↓ eviction if full
메모리 저장
```

## 구현 상태

### ✅ 완료
- [x] Error types
- [x] Protocol definitions (Module, Controller)
- [x] JSON line codec
- [x] SIEVE cache algorithm
- [x] Thread-safe data layer
- [x] Topic pattern matching (wildcards)
- [x] Subscription registry
- [x] Sequential message bus
- [x] Module process spawning
- [x] Module registry
- [x] Module manager (메시지 처리)
- [x] IPC server (TCP)
- [x] Command handler
- [x] Configuration management
- [x] Module message processing loop
  - Module stdout → Bus 연결
  - Module stdout → Data Layer 연결
  - Bus → Module stdin 연결
- [x] Lifecycle management
  - Graceful shutdown (configurable)
- [x] Example modules (Rust, Python)
- [x] Integration tests
- [x] Documentation

### 🚧 향후 개선 사항
- [ ] Module crash recovery
  - Module crash 감지
- [ ] Health check 구현
- [ ] 동적 로딩 (runtime module 추가/제거)

### 📋 향후 계획
- [ ] Controller 클라이언트 라이브러리
- [ ] 벤치마크
- [ ] Docker 이미지
- [ ] systemd 통합
- [ ] Web UI

## 테스트 전략

### Unit Tests (각 파일 내부)
- SIEVE eviction 정확성
- Topic pattern matching
- 메시지 직렬화/역직렬화

### Integration Tests (tests/)
- Bus pub/sub 종단간 테스트
- Data layer CRUD
- SIEVE eviction 통합

### Example Tests (향후)
- 실제 모듈과 Daemon 통신
- 다중 모듈 시나리오
- 고부하 테스트

## 빌드 아티팩트

```
target/
├── debug/
│   ├── daemon_v1                  # Debug 빌드
│   └── examples/
│       └── echo_module            # 예제 모듈
└── release/
    └── daemon_v1                  # Release 빌드 (최적화)
```

## 로그 레벨

```bash
# Info (기본)
RUST_LOG=info cargo run

# Debug (상세)
RUST_LOG=debug cargo run

# Trace (모든 것)
RUST_LOG=trace cargo run

# 특정 모듈만
RUST_LOG=daemon_v1::bus=debug cargo run
```
