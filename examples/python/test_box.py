from blink import Box

def main():
    print("--- Blink SDK Boxlite Example ---")
    
    with Box(image="python-3.11") as box:
        print("Box initialized. Running untrusted code...")
        
        # This code will be injected and executed inside the sandboxed environment
        code_to_run = """
print('Hello from the Blink Sandbox!')
import math
print(f'Math test: PI is {math.pi}')

# Simulate an error
raise ValueError('Intentional Sandbox Crash')
        """
        
        try:
            result = box.run(code_to_run)
            print("\n[Host] Execution finished!")
            print(f"Exit Code: {result.exit_code}")
            print(f"Stdout:\n{result.stdout}")
            print(f"Stderr:\n{result.stderr}")
        except Exception as e:
            print(f"[Host] Failed to execute in Box: {e}")

if __name__ == "__main__":
    main()
