# Rust Simulate Scheduling OS Processes
Simulate the OS processes scheduling, it can run "heavy work"(just by thread::slepp()) a whlie, and invoke the file IO operations randomly. Some other properties waiting to be discovered. 😁

# Run

## Optional
You can first start a terminal for running the [echo server](https://github.com/wyhhh/echo_server), that makes the messages output.

`cargo r --release -- localhost:9999`

## Main
And then, you start this project from another terminal:

`cargo r --release -- localhost:9999`

You can also change the value of `const RANDOM_PROCESSES: usize = 10;` in `main.rs` for another working threads.

![alt text](https://github.com/wyhhh/simulate_schedule/blob/master/show.png)