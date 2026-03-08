#!/usr/bin/env python3
"""
Echo Module - Python Example
Demonstrates Module <-> Daemon protocol implementation in Python
"""

import sys
import json
import logging

# Setup logging to stderr (stdout is for protocol communication)
logging.basicConfig(
    level=logging.INFO,
    format='[%(levelname)s] %(message)s',
    stream=sys.stderr
)

def send_message(msg):
    """Send JSON message to Daemon via stdout"""
    json_str = json.dumps(msg)
    print(json_str, flush=True)

def handle_init(msg):
    """Handle init command"""
    module_name = msg.get('module_name', 'unknown')
    config = msg.get('config', {})

    logging.info(f"Echo module '{module_name}' initialized")
    logging.info(f"Config: {config}")

    # Subscribe to echo.* topics
    send_message({
        'type': 'subscribe_request',
        'topic': 'echo.*'
    })

    # Log initialization
    send_message({
        'type': 'log',
        'message': f"Echo module '{module_name}' ready",
        'level': 'info'
    })

def handle_command(msg):
    """Handle command from Controller"""
    cmd_id = msg.get('id', 'unknown')

    # Send ACK
    send_message({
        'type': 'ack',
        'id': cmd_id
    })

    # Echo command as event
    send_message({
        'type': 'publish',
        'topic': 'echo.response',
        'metadata': {
            'original_id': cmd_id,
            'echoed': msg
        }
    })

def handle_event(msg):
    """Handle event from Message Bus"""
    topic = msg.get('topic', '')
    data = msg.get('data')

    logging.info(f"Received event: {topic}")

    # Echo back with modified topic
    if data is not None:
        new_topic = topic.replace('echo.', 'echo.response.')
        send_message({
            'type': 'publish',
            'topic': new_topic,
            'metadata': data
        })

def handle_shutdown(msg):
    """Handle shutdown command"""
    logging.info("Echo module shutting down")
    send_message({
        'type': 'log',
        'message': 'Echo module shutdown complete',
        'level': 'info'
    })
    return True  # Signal to exit

def main():
    """Main loop - read from stdin and process messages"""
    logging.info("Echo module starting...")

    try:
        for line in sys.stdin:
            line = line.strip()
            if not line:
                continue

            try:
                msg = json.loads(line)
                cmd = msg.get('cmd', '')

                if cmd == 'init':
                    handle_init(msg)
                elif cmd == 'command':
                    handle_command(msg)
                elif cmd == 'event':
                    handle_event(msg)
                elif cmd == 'shutdown':
                    if handle_shutdown(msg):
                        break
                else:
                    logging.warning(f"Unknown command: {cmd}")

            except json.JSONDecodeError as e:
                logging.error(f"Failed to parse JSON: {e}")
            except Exception as e:
                logging.error(f"Error processing message: {e}")

    except KeyboardInterrupt:
        logging.info("Interrupted by user")
    except Exception as e:
        logging.error(f"Fatal error: {e}")
        return 1

    logging.info("Echo module stopped")
    return 0

if __name__ == '__main__':
    sys.exit(main())
