import socket
import struct
import json
import time
import sys
import io
from contextlib import redirect_stdout, redirect_stderr

# Linux AF_VSOCK is 40
AF_VSOCK = getattr(socket, "AF_VSOCK", 40)
VMADDR_CID_HOST = 2
VHUB_PORT = 10000

# Vsock Packet Header Format
# magic(u32) + version(u8) + msg_type(u8) + flags(u16) + payload_len(u32) + request_id(u64)
HEADER_FORMAT = "<I B B H I Q"
HEADER_SIZE = struct.calcsize(HEADER_FORMAT)
BLINK_MAGIC = 0x424C494E

class MessageType:
    Handshake = 0x01
    Heartbeat = 0x02
    Error = 0x03
    RpcRequest = 0x10
    RpcResponse = 0x11
    RpcError = 0x12

class VsockClient:
    def __init__(self, cid, port):
        self.cid = cid
        self.port = port
        self.sock = socket.socket(AF_VSOCK, socket.SOCK_STREAM)
        self.request_id_counter = 1

    def connect(self):
        self.sock.connect((self.cid, self.port))
        print(f"[Agent] Connected to V-Hub on Host (CID: {self.cid}, Port: {self.port})")

    def send_message(self, msg_type: int, payload: bytes):
        req_id = self.request_id_counter
        self.request_id_counter += 1
        
        header = struct.pack(
            HEADER_FORMAT,
            BLINK_MAGIC,
            1, # version
            msg_type,
            0, # flags
            len(payload),
            req_id
        )
        self.sock.sendall(header + payload)
        return req_id

    def recv_message(self):
        # Read exactly the header
        header_data = self._recvall(HEADER_SIZE)
        if not header_data:
            return None
            
        magic, version, msg_type, flags, payload_len, req_id = struct.unpack(HEADER_FORMAT, header_data)
        
        if magic != BLINK_MAGIC:
            raise ValueError(f"Invalid magic number: {magic:#x}")
            
        payload = self._recvall(payload_len)
        return msg_type, req_id, payload

    def _recvall(self, n):
        data = bytearray()
        while len(data) < n:
            packet = self.sock.recv(n - len(data))
            if not packet:
                return None
            data.extend(packet)
        return bytes(data)

    def close(self):
        self.sock.close()

def execute_and_report(client: VsockClient, code: str):
    f_out = io.StringIO()
    f_err = io.StringIO()
    exit_code = 0
    
    # Intercept standard output and errors
    with redirect_stdout(f_out), redirect_stderr(f_err):
        try:
            exec(code)
        except Exception as e:
            print(f"Execution Error: {e}", file=sys.stderr)
            import traceback
            traceback.print_exc(file=sys.stderr)
            exit_code = 1
            
    # Assemble structured JSON
    payload = {
        "event": "execution_result",
        "stdout": f_out.getvalue(),
        "stderr": f_err.getvalue(),
        "exit_code": exit_code,
        "metrics": {"memory_mb": 42} # Placeholder for future expansion
    }
    
    print(f"[Agent] Reporting execution back to Host...")
    client.send_message(MessageType.RpcRequest, json.dumps(payload).encode('utf-8'))
    
    # Wait for acknowledgment
    msg_type, req_id, response_payload = client.recv_message()
    if msg_type == MessageType.RpcResponse:
        response_data = json.loads(response_payload.decode('utf-8'))
        print(f"[Agent] Host Acknowledged: {response_data}")

def main():
    print("[Agent] Booting up inside Blink Sandbox...")
    
    client = VsockClient(VMADDR_CID_HOST, VHUB_PORT)
    try:
        client.connect()
        
        # 1. Send Handshake
        client.send_message(MessageType.Handshake, b"")
        msg_type, req_id, payload = client.recv_message()
        print(f"[Agent] Handshake response: {payload.decode()}")
        
        # 2. Execute unsafe agent script and report structured output
        unsafe_code = """
import time
print("Step 1: Starting Agent Task...")
time.sleep(0.5)
print("Step 2: Processing Data...")
# Simulate an intentional error
raise ValueError("Simulated Agent Crash")
        """
        execute_and_report(client, unsafe_code)
        
        # Keep connection alive for a brief moment
        time.sleep(1)
        client.close()
        print("[Agent] Disconnected. Shutting down.")
        
    except Exception as e:
        print(f"[Agent] Error communicating with V-Hub: {e}")

if __name__ == "__main__":
    main()
