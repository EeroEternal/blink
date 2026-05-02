import subprocess
import json
import tempfile
import os
import sys
from typing import Optional

# This template runs INSIDE the libkrun VM.
# It uses AF_VSOCK to report execution results back to the Host.
AGENT_TEMPLATE = """
import socket
import struct
import json
import sys
import io
import time
from contextlib import redirect_stdout, redirect_stderr

AF_VSOCK = getattr(socket, "AF_VSOCK", 40)
VMADDR_CID_HOST = 2
VHUB_PORT = 10000

HEADER_FORMAT = "<I B B H I Q"
HEADER_SIZE = struct.calcsize(HEADER_FORMAT)
BLINK_MAGIC = 0x424C494E

class MessageType:
    Handshake = 0x01
    RpcRequest = 0x10
    RpcResponse = 0x11

class VsockClient:
    def __init__(self, cid, port):
        self.sock = socket.socket(AF_VSOCK, socket.SOCK_STREAM)
        self.cid = cid
        self.port = port
        self.req_id = 1

    def connect(self):
        self.sock.connect((self.cid, self.port))

    def send(self, msg_type, payload):
        req_id = self.req_id
        self.req_id += 1
        header = struct.pack(HEADER_FORMAT, BLINK_MAGIC, 1, msg_type, 0, len(payload), req_id)
        self.sock.sendall(header + payload)

    def recv(self):
        hdr = self._recvall(HEADER_SIZE)
        if not hdr: return None
        magic, ver, msg_type, flags, plen, req_id = struct.unpack(HEADER_FORMAT, hdr)
        return msg_type, req_id, self._recvall(plen)

    def _recvall(self, n):
        data = bytearray()
        while len(data) < n:
            packet = self.sock.recv(n - len(data))
            if not packet: return None
            data.extend(packet)
        return bytes(data)
        
    def close(self):
        self.sock.close()

def main():
    client = VsockClient(VMADDR_CID_HOST, VHUB_PORT)
    try:
        client.connect()
        client.send(MessageType.Handshake, b"")
        client.recv()
        
        f_out = io.StringIO()
        f_err = io.StringIO()
        exit_code = 0
        
        # User code injected by Boxlite SDK
        user_code = {user_code_repr}
        
        with redirect_stdout(f_out), redirect_stderr(f_err):
            try:
                exec(user_code, {{"__name__": "__main__"}})
            except Exception as e:
                import traceback
                traceback.print_exc(file=sys.stderr)
                exit_code = 1
                
        payload = {{
            "event": "execution_result",
            "stdout": f_out.getvalue(),
            "stderr": f_err.getvalue(),
            "exit_code": exit_code
        }}
        
        client.send(MessageType.RpcRequest, json.dumps(payload).encode('utf-8'))
        client.recv() # Acknowledge
        client.close()
    except Exception as e:
        print(f"Agent Boot Error: {{e}}", file=sys.stderr)

if __name__ == "__main__":
    main()
"""

class ExecutionResult:
    def __init__(self, stdout: str, stderr: str, exit_code: int):
        self.stdout = stdout
        self.stderr = stderr
        self.exit_code = exit_code

    def __repr__(self):
        return f"<ExecutionResult exit_code={self.exit_code} stdout={repr(self.stdout)}>"

class Box:
    """
    Blink SDK - The 'Box' paradigm.
    Abstracts away VM instantiation, environment pass-through, and Vsock networking.
    """
    def __init__(self, image: str = "python-3.11", cli_path: str = "./target/debug/blink-cli"):
        self.image = image
        self.cli_path = cli_path
        self._temp_files = []

    def __enter__(self):
        # In a real environment, this might initialize a warm-pool instance or prepare tmpfs
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        # Clean up temporary injected scripts from the host
        for f in self._temp_files:
            try:
                os.remove(f)
            except OSError:
                pass

    def run(self, code: str) -> ExecutionResult:
        """
        Spawns the Blink VM, injects the code, runs it inside the sandboxed environment,
        and cleanly returns the structured ExecutionResult.
        """
        # 1. Prepare guest payload containing the user's code + Vsock reporter
        guest_script_content = AGENT_TEMPLATE.format(user_code_repr=repr(code))
        
        fd, temp_path = tempfile.mkstemp(suffix=".py", prefix="blink_agent_")
        with os.fdopen(fd, 'w') as f:
            f.write(guest_script_content)
        self._temp_files.append(temp_path)
        
        try:
            # 2. Execute Blink CLI Subprocess Mode
            # It will boot the VM, wait for the guest to report over Vsock, and print purely JSON.
            result = subprocess.run(
                [self.cli_path, "run", temp_path],
                capture_output=True,
                text=True,
                check=True
            )
            
            output = result.stdout.strip()
            if not output:
                raise RuntimeError("Blink CLI returned empty output. Check stderr for boot failures.")
                
            payload = json.loads(output)
            
            return ExecutionResult(
                stdout=payload.get("stdout", ""),
                stderr=payload.get("stderr", ""),
                exit_code=payload.get("exit_code", 1)
            )
            
        except subprocess.CalledProcessError as e:
            # This captures crashes of the Zig host itself (e.g. libkrun initialization failures)
            print(f"Blink Engine Crash:\n{e.stderr}", file=sys.stderr)
            raise
