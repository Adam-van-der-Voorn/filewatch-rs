#!/usr/bin/env python3
from datetime import datetime
import os
import time
import threading

DELAY_MUL = 3

def load_template_lines(template_file):
    with open(template_file, 'r') as f:
        lines = [line.rstrip('\n') for line in f.readlines()]
    return lines

def log(tmpl_path, out_path, delays):
    try:
        template_data = load_template_lines(tmpl_path)            
        while True:
            with open(out_path, 'a') as f:
                for i in range(len(template_data)):
                    line = template_data[i]
                    delay = delays[i % len(delays)] * DELAY_MUL
                    time.sleep(delay)
                    dt = datetime.fromtimestamp(time.time())
                    # Format the datetime as a string
                    ts = dt.strftime('%Y-%m-%d %H:%M:%S')
                    f.write(f"{ts} {line}\n")
                    f.flush()
                    print(f"Logged to {out_path}: {line}")

            
                
            # Clear the output file
            with open(out_path, 'w') as f:
                pass
            print(f"Cleared {out_path}")
    except KeyboardInterrupt:
        print("\nLogging stopped")

if __name__ == "__main__":
    current_dir = os.path.dirname(os.path.abspath(__file__))
    delays_1 = [0.5, 1.2, 0.8, 2.1, 0.3, 1.7, 0.9, 1.5, 0.6, 2.0]
    thread_1 = threading.Thread(target=log, args=(f"{current_dir}/tmpl/1.log", f"{current_dir}/out/1-out.log", delays_1))
    thread_1.start()

    time.sleep(1 * DELAY_MUL)

    delays_2 = [1.1, 0.4, 1.8, 0.7, 2.3, 0.9, 1.3, 0.5, 1.6, 0.8]
    thread_2 = threading.Thread(target=log, args=(f"{current_dir}/tmpl/2.log", f"{current_dir}/out/2-out.log", delays_2))
    thread_2.start()