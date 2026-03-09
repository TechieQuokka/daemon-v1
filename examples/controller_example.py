#!/usr/bin/env python3
"""
Controller Example - Python
Demonstrates how to communicate with Daemon V1 via TCP
"""

import socket
import json
import time


class DaemonClient:
    """Daemon V1 Controller 클라이언트"""

    def __init__(self, host='127.0.0.1', port=9000):
        self.host = host
        self.port = port
        self.sock = None
        self._req_id = 0

    def connect(self):
        """Daemon에 연결"""
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.connect((self.host, self.port))
        print(f"✓ Connected to Daemon at {self.host}:{self.port}")

    def close(self):
        """연결 종료"""
        if self.sock:
            self.sock.close()
            print("✓ Connection closed")

    def _send_request(self, action, params=None):
        """요청 전송 및 응답 수신"""
        self._req_id += 1
        request = {
            'action': action,
            'params': params,
            'id': f'req-{self._req_id}'
        }

        # 요청 전송
        request_json = json.dumps(request) + '\n'
        self.sock.sendall(request_json.encode())
        print(f"\n→ Request: {action}")
        print(f"  {json.dumps(request, indent=2)}")

        # 응답 수신
        response_data = self.sock.recv(4096).decode().strip()
        response = json.loads(response_data)
        print(f"← Response:")
        print(f"  {json.dumps(response, indent=2)}")

        return response

    # Module Management

    def start_module(self, name, path, config=None):
        """Module 시작"""
        return self._send_request('module.start', {
            'name': name,
            'path': path,
            'config': config or {}
        })

    def stop_module(self, module_id, timeout=5000):
        """Module 종료"""
        return self._send_request('module.stop', {
            'id': module_id,
            'timeout': timeout
        })

    def list_modules(self):
        """Module 목록"""
        return self._send_request('module.list')

    def health_check(self, module_id):
        """Module 상태 확인"""
        return self._send_request('health_check', {
            'module': module_id
        })

    def send_command(self, module_id, command_id, payload):
        """Module에 명령 전송"""
        params = {
            'module': module_id,
            'id': command_id,
            **payload  # payload의 모든 필드를 params에 병합
        }
        return self._send_request('module.command', params)

    # Data Layer

    def get_data(self, key):
        """데이터 읽기"""
        return self._send_request('data.get', {'key': key})

    def set_data(self, key, value=None, path=None):
        """데이터 저장"""
        params = {'key': key}
        if value is not None:
            params['value'] = value
        elif path is not None:
            params['path'] = path
        else:
            raise ValueError("Either value or path must be provided")

        return self._send_request('data.set', params)

    def delete_data(self, key):
        """데이터 삭제"""
        return self._send_request('data.delete', {'key': key})

    def list_keys(self):
        """모든 키 조회"""
        return self._send_request('data.list')

    # Message Bus

    def publish(self, topic, data):
        """이벤트 발행"""
        return self._send_request('bus.publish', {
            'topic': topic,
            'data': data
        })

    # Daemon Management

    def daemon_status(self):
        """Daemon 상태"""
        return self._send_request('daemon.status')

    def daemon_shutdown(self):
        """Daemon 종료"""
        return self._send_request('daemon.shutdown')


def demo_basic_operations(client):
    """기본 동작 데모"""
    print("\n" + "="*60)
    print("DEMO 1: Basic Operations")
    print("="*60)

    # 1. Daemon 상태 확인
    print("\n[1] Daemon Status")
    status = client.daemon_status()

    # 2. Data Layer 사용
    print("\n[2] Data Layer Operations")
    client.set_data('user_count', 42)
    client.set_data('config', {
        'host': 'localhost',
        'port': 8080,
        'debug': True
    })

    result = client.get_data('user_count')
    print(f"   Retrieved: {result}")

    keys = client.list_keys()
    print(f"   All keys: {keys}")

    # 3. Message Bus 사용
    print("\n[3] Message Bus")
    client.publish('system.test', {
        'message': 'Hello from Controller',
        'timestamp': time.time()
    })


def demo_module_lifecycle(client):
    """Module 생명주기 데모"""
    print("\n" + "="*60)
    print("DEMO 2: Module Lifecycle")
    print("="*60)

    # 참고: 실제 Module 실행 파일이 필요합니다
    # 여기서는 예시로만 표시

    module_path = "./target/debug/examples/echo_module"

    print("\n[1] Start Module")
    print(f"   (Skipped - module path: {module_path})")
    # result = client.start_module('echo', module_path, {
    #     'data_layer_path': '/data_layer'
    # })

    print("\n[2] List Modules")
    modules = client.list_modules()

    print("\n[3] Health Check")
    print("   (Skipped - no running modules)")
    # health = client.health_check('echo')

    print("\n[4] Stop Module")
    print("   (Skipped)")
    # client.stop_module('echo')


def demo_data_workflow(client):
    """데이터 워크플로우 데모"""
    print("\n" + "="*60)
    print("DEMO 3: Data Workflow")
    print("="*60)

    # 시나리오: 작업 큐 시뮬레이션

    print("\n[1] Create Task Queue")
    tasks = [
        {'id': 1, 'type': 'process', 'priority': 'high'},
        {'id': 2, 'type': 'analyze', 'priority': 'medium'},
        {'id': 3, 'type': 'report', 'priority': 'low'},
    ]
    client.set_data('task_queue', tasks)

    print("\n[2] Publish Task Event")
    client.publish('task.created', {
        'count': len(tasks),
        'source': 'controller'
    })

    print("\n[3] Read Task Queue")
    result = client.get_data('task_queue')

    print("\n[4] Mark as Processed")
    client.set_data('task_queue_processed', True)
    client.publish('task.completed', {
        'count': len(tasks)
    })

    print("\n[5] Cleanup")
    client.delete_data('task_queue')
    client.delete_data('task_queue_processed')


def main():
    """메인 함수"""
    print("="*60)
    print("Daemon V1 Controller Example")
    print("="*60)

    client = DaemonClient()

    try:
        # 연결
        client.connect()

        # 데모 실행
        demo_basic_operations(client)
        demo_module_lifecycle(client)
        demo_data_workflow(client)

        print("\n" + "="*60)
        print("All demos completed!")
        print("="*60)

    except ConnectionRefusedError:
        print("\n✗ Error: Cannot connect to Daemon")
        print("  Make sure Daemon is running:")
        print("  $ cargo run")
        return 1

    except Exception as e:
        print(f"\n✗ Error: {e}")
        import traceback
        traceback.print_exc()
        return 1

    finally:
        client.close()

    return 0


if __name__ == '__main__':
    import sys
    sys.exit(main())
