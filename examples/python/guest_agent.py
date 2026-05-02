import socket
import struct
import json
import time

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

def main():
    print("[Agent] Booting up inside Blink Sandbox...")
    
    client = VsockClient(VMADDR_CID_HOST, VHUB_PORT)
    try:
        client.connect()
        
        # 1. Send Handshake
        client.send_message(MessageType.Handshake, b"")
        msg_type, req_id, payload = client.recv_message()
        print(f"[Agent] Handshake response: {payload.decode()}")
        
        # 2. Send JSON RPC Request
        rpc_payload = {
            "method": "tool.execute",
            "params": {
                "tool": "python",
                "code": "print('hello from sandbox')"
            },
            "timeout_ms": 30000
        }
        
        print(f"[Agent] Sending RPC Request...")
        client.send_message(MessageType.RpcRequest, json.dumps(rpc_payload).encode('utf-8'))
        
        # Wait for RPC response
        msg_type, req_id, payload = client.recv_message()
        if msg_type == MessageType.RpcResponse:
            response_data = json.loads(payload.decode('utf-8'))
            print(f"[Agent] RPC Response: {response_data}")
        
        # Keep connection alive for a brief moment
        time.sleep(1)
        client.close()
        print("[Agent] Disconnected. Shutting down.")
        
    except Exception as e:
        print(f"[Agent] Error communicating with V-Hub: {e}")

if __name__ == "__main__":
    main()
