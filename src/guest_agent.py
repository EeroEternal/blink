import socket
import time

# Linux AF_VSOCK is 40
AF_VSOCK = getattr(socket, "AF_VSOCK", 40)
VMADDR_CID_HOST = 2
VHUB_PORT = 10000

def main():
    print("[Agent] Booting up inside Blink Sandbox...")
    
    # Connect to the V-Hub on the host
    try:
        s = socket.socket(AF_VSOCK, socket.SOCK_STREAM)
        s.connect((VMADDR_CID_HOST, VHUB_PORT))
        print(f"[Agent] Connected to V-Hub on Host (CID: {VMADDR_CID_HOST}, Port: {VHUB_PORT})")
        
        # Send Handshake
        msg = b"Hello, Host!"
        s.sendall(msg)
        print(f"[Agent] Sent: {msg.decode()}")
        
        # Wait for reply
        reply = s.recv(1024)
        print(f"[Agent] Received from V-Hub: {reply.decode()}")
        
        # Keep connection alive for a brief moment to simulate work
        time.sleep(1)
        s.close()
        print("[Agent] Disconnected. Shutting down.")
        
    except Exception as e:
        print(f"[Agent] Error connecting to V-Hub: {e}")

if __name__ == "__main__":
    main()
