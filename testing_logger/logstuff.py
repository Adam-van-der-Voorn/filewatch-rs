#!/usr/bin/env python3
import time
import heapq

def load_template_lines(template_file, file_index):
    """Load all lines from a template file with predefined delays"""
    with open(template_file, 'r') as f:
        lines = [line.rstrip('\n') for line in f.readlines()]
    
    # Predefined delays for each line (in seconds)
    if file_index == 0:  # 1.log
        delays = [0.5, 1.2, 0.8, 2.1, 0.3, 1.7, 0.9, 1.5, 0.6, 2.0]
    else:  # 2.log  
        delays = [1.1, 0.4, 1.8, 0.7, 2.3, 0.9, 1.3, 0.5, 1.6, 0.8]
    
    # Ensure we have enough delays for all lines
    while len(delays) < len(lines):
        delays.extend(delays)
    
    return list(zip(lines, delays[:len(lines)]))

def main():
    base_dir = "/home/adamv/proj/filewatch-rs/testing_logger"
    templates = [
        f"{base_dir}/tmpl/1.log",
        f"{base_dir}/tmpl/2.log"
    ]
    outputs = [
        f"{base_dir}/out/1.log", 
        f"{base_dir}/out/2.log"
    ]
    
    # Different start times for each file (in seconds from start)
    file_start_offsets = [0.0, 3.5]  # file 1 starts immediately, file 2 starts after 3.5s
    
    print("Starting time-based log replay")
    print("Press Ctrl+C to stop")
    
    try:
        while True:
            # Load template data and create scheduled events
            events = []  # (time, event_type, file_index, line_or_none)
            start_time = time.time()
            
            for file_index, template in enumerate(templates):
                # Schedule file clear event
                clear_time = start_time + file_start_offsets[file_index]
                heapq.heappush(events, (clear_time, 'clear', file_index, None))
                
                # Schedule log events for this file
                template_data = load_template_lines(template, file_index)
                current_time = file_start_offsets[file_index]
                
                for line, delay in template_data:
                    current_time += delay
                    heapq.heappush(events, (start_time + current_time, 'log', file_index, line))
            
            # Process events in chronological order
            while events:
                event_time, event_type, file_index, line = heapq.heappop(events)
                
                # Wait until it's time for this event
                current_time = time.time()
                if event_time > current_time:
                    time.sleep(event_time - current_time)
                
                if event_type == 'clear':
                    # Clear the output file
                    output_file = outputs[file_index]
                    with open(output_file, 'w') as f:
                        pass
                    print(f"Cleared {output_file}")
                elif event_type == 'log':
                    # Write the log line
                    output_file = outputs[file_index]
                    with open(output_file, 'a') as f:
                        f.write(line + '\n')
                    print(f"Logged to {output_file}: {line}")
            
            print("\n--- Cycle complete, restarting ---\n")
    except KeyboardInterrupt:
        print("\nLogging stopped")

if __name__ == "__main__":
    main()