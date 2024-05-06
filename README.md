# Async Rust - TCP Server

- [Overview](#overview)
- [Concurrency paradigm](#concurrency-paradigm)
  - [Asynchronous programming](#asynchronous-programming)
  - [Message passing](#message-passing)
- [Achieved level of concurrency](#achieved-level-of-concurrency)
  - [Client-level concurrency](#client-level-concurrency)
  - [Task-level concurrency](#task-level-concurrency)
  - [Message passing implementation detail](#message-passing-implementation-detail)
- [Other approaches](#other-approaches)
- [Breakdown of performance improvements](#breakdown-of-performance-improvements)
- [References](#references)

## Overview

The implementation of the TCP server heavily relies on Tokio to enable parallelism between the server and the client handler which creates the specified amount of clients and connects to the server. Tokio’s unbounded channel is also used as message passing was explored in an attempt to improve the performance of the TCP server. Caching was also introduced to ideally prevent unnecessary recalculation and aims to speed up I/O-bound tasks where possible.

The default `tokio::main` macro was used as the assumption that was made was that it would be able to provide the best performance as the [default flavour](https://docs.rs/tokio-macros/latest/tokio_macros/attr.main.html#multi-threaded-runtime) is set to `multi_thread`, with the maximum number of CPUs as the number of worker threads. This also allowed the program to run on different machines for testing without needing to modify machine-specific values.

## Concurrency paradigm

The following paradigms, accompanied with a brief contextual explanation, were implemented in the TCP server in decreasing order of performance improvements.

### Asynchronous programming

Tokio threads were used with async functions that return futures to allow the TCP server to handle multiple clients concurrently and has the ability to process multiple requests from the same client when paired with message passing. The Tokio runtime would take the maximum amount of worker threads the system has and does the necessary scheduling and yielding of threads, typically for I/O-bound tasks, to maximise the efficiency of every available thread thereby achieving asynchronous non-blocking operations between clients. Given that there are multiple threads, these actions are also done in parallel. To summarise, this pattern is what mainly drives client and task-level concurrency.

### Message passing

Tokio’s unbounded channels enable the TCP server to process all of a client’s request concurrently by separating the reading and writing to a `TcpStream` which is unique to a client. In theory, this can allow multiple requests for the same client to run in parallel. However, such performance improvements were not observed as clients send requests to the server in a sequentially blocking way – only sending the next request after receiving the response of the current request.

## Achieved level of concurrency

The TCP server was able to achieve task-level concurrency, but in order to reach that milestone, client-level concurrency is required. The following explanations omit the changes that were made to the skeleton code that mainly converts the some standard library crates to Tokio’s (e.g. `task`, `TcpListener`, etc.) and converting normal functions and closures to their async variants.

### Client-level concurrency

In the `start_server` function of [`server.rs`](./src/server.rs), the skeleton code initially loops over `TcpListener` and waits for new incoming `TcpStream`. When one exists, it then runs the `handle_connection` function on that stream using the same thread without yielding at any point in time. This is inefficient as more clients may connect to the TCP server while it is busy handling the stream that came before it and blocks until completion before processing the next one despite each stream being completely isolated from each other – there is no shared state between streams and no need for synchronisation.

To circumvent this issue, a Tokio thread is spawned for every stream that connects to the TCP server. By doing so, the TCP server can allow multiple clients to connect to the TCP server at once.

### Task-level concurrency

Client-level concurrency still leaves a set of problems that needs to be addressed – the processing of tasks. The skeleton code was originally using the `execute` function in [`task.rs`](./src/task.rs) which is a normal function that returns a `u8` value. This is not ideal as some tasks are I/O intensive which is simulated by sleeping for a certain duration. During this time the thread goes into a sleep-lock state which is not ideal. While waiting for the I/O operation to complete, it could be better used to process the results for requests from other clients.

To fix this issue, the `execute` function that was originally used in the `get_task_value` function in [`server.rs`](./src/server.rs) is replaced with `execute_async`, along with the necessary changes needed for async functions to work. Now when the `get_task_value` is called, we can `await` on it as it now returns a future which allows the thread to yield and let the Tokio runtime decide how to best utilise this free thread until the result of the future is ready to be written back to the client via their respective `TcpStream`s. At this stage, a `task_semaphore` which originates from the `start_server` function is passed all the way down to the `get_task_value` function in order to allow up to 40 CPU-intensive tasks to run at once, while allowing other types task to process without constraint. A cache was also introduced at the same point to store values for unique tasks to prevent unnecessary recalculation and would greatly benefit overlapping I/O tasks.

Through this, tasks from different clients are able to execute in parallel, but tasks from the same client cannot be executed in parallel as `client.rs` requires the server to respond to a client’s initial request before the same client sends another.

### Message passing implementation detail

Finalising the implementation of the server, the message passing paradigm was included as it enables requests from the same client to be processed concurrently. This was achieved in `handle_connection` by introducing an unbounded channel and spawning a Tokio thread that listens on the receiving-end of the channel and only returns (becomes joinable) when the channel is closed. A new Tokio thread spawns for every new request, processes it, and sends the result to the receiver thread. The `handle_connection` function only exits when the thread that listens to the receiver-end of the channel becomes joinable. This happens when the channel closes after receiving the last request from the client and all of the results have been written back to the client. However, this did not bring about its expected performance improvements due to the request-sending behaviour of each client.

## Other approaches

Before implementing message passing, a general approach that was explored while formulating new ways to improve the performance of the TCP server was to essentially collect all the request processing and write-backs as futures and process all of them at once using [`join_all`](https://docs.rs/futures/latest/futures/future/fn.join_all.html) or Tokio’s [JoinSet](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html). This eventually led to a blocked state due to the behaviour of [`client.rs`](./src/client.rs) and message passing was implemented to allow progress to be made as well as theoretically achieve concurrent request processing from the same client.

## Breakdown of performance improvements

The following shows how the running time of the TCP server improved with every iteration.

- Machine: Macbook Pro M3, 12 cores
- Command: `cargo run -r 8080 1 40 20`

| **Concurrency level** | **Elapsed time (s)** |
| --------------------- | -------------------- |
| No concurrency        | 215.359335314        |
| Client-level          | 17.362779771s        |
| Task-level            | 14.617838871s        |
| Caching               | 9.348029125s         |

> [!NOTE]
> The performance gained from caching is highly dependent on how the random numbers are generated in this scenario. Numbers generated for a small number type (e.g. `u8`) would benefit from caching as there is more likely to be a cache hit as opposed to a larger number type (e.g. `u64`)

## References

- [Understanding event loops (JavaScript)](https://towardsdev.com/event-loop-in-javascript-672c07618dc9)
  - Some of these concepts can extend to Rust, but with multi-threading
- [Multithreaded server and thread pools](https://doc.rust-lang.org/book/ch20-02-multithreaded.html)
- [Tokio tasks](https://tokio.rs/tokio/tutorial/spawning#tasks)
- [Actors with Tokio](https://ryhl.io/blog/actors-with-tokio/)
  - Not implemented but inspired the use of message passing
