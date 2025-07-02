# Payments Engine

This project is a simple transaction processing engine written in Rust. It reads a stream of transactions from a CSV file, processes them to update client account states, and outputs the final state of all accounts to a new CSV file, as per the coding challenge specification.

## How to Run

The application is a standard Cargo project and is run from the command line.

1.  **Build the project in release mode for optimal performance:**
    ```sh
    cargo build --release
    ```

2.  **Run the engine:**
    The program takes one command-line argument: the path to the input CSV file. It writes the resulting account states to standard output. It is recommended to redirect this output to a file.

    ```sh
    cargo run --release -- transactions.csv > accounts.csv
    ```
    A comprehensive test file, `transactions.csv`, is included in the repository to validate all functionalities and edge cases.

## Design Decisions

### 1. Core Architecture: Decoupled Streaming Processor

The core logic is encapsulated within the `PaymentEngine` struct, which acts as a self-contained library. The main application binary (`main.rs`) is a simple harness that drives this engine, feeding it records from a file.

This design has two major advantages:
- **Streaming:** The engine processes one `InputRecord` at a time. The `csv` crate is configured to read the input file as a stream (with comment support for lines starting with `#`), ensuring the entire dataset is never loaded into memory. This results in a low, constant memory footprint, regardless of file size.
- **Decoupling:** The `engine` module has no knowledge of files or standard I/O. It operates purely on data structures (`InputRecord`) and returns `Result`s. This makes the core logic highly portable, testable, and ready to be "bundled" into other applications, such as a server.

### 2. Data Integrity and Precision

- **`rust_decimal`:** To ensure absolute correctness in financial calculations, the `rust_decimal` crate is used for all monetary values. This avoids the precision and rounding errors inherent in standard floating-point types, which are unsuitable for financial systems.
- **Guaranteed Four-Decimal Output:** A custom `serde` serialization function, `serialize_with_four_decimals`, is used to format all output monetary values to exactly four decimal places (e.g., `1.5000`). This function both rounds the `Decimal` type mathematically and formats the string representation, strictly adhering to the output specification.

### 3. Robustness and Error Handling

- **Custom `Error` Enum**: The engine's processing functions return a `Result<(), EngineError>`. A detailed `EngineError` enum captures all possible logical failures (e.g., `InsufficientFunds`, `AccountLocked`, `TransactionNotDisputed`). This makes the engine's behavior explicit and testable.
- **Encapsulation**: The `Account` struct's fields are private. State modifications are only possible through methods that enforce business rules (e.g., an account cannot be overdrawn), preventing the engine from ever reaching an invalid state.
- **Graceful Failure & Idempotency**: The engine is designed to be resilient to invalid data, as might be expected from a partner system. Invalid references (e.g., a dispute for a non-existent transaction or a client) are handled by returning a descriptive error, which is logged to `stderr` without crashing the program. Operations like disputes are idempotent; processing the same dispute twice will not corrupt the account's state.

## Design for Concurrency (Server Readiness)

The `PaymentEngine` is `Send` but not `Sync`. This means it can be safely moved between threads, but not accessed by multiple threads at the same time. This is the ideal setup for the standard Rust concurrency pattern for shared mutable state: `Arc<Mutex<T>>`.

A server would wrap the engine like this:

```rust
use std::sync::{Arc, Mutex};
use crate::engine::PaymentEngine; // Assuming our engine module

// In the server's initialization code:
let engine = PaymentEngine::new();
let shared_engine = Arc::new(Mutex::new(engine));

// For each incoming connection, the server would spawn a task:
tokio::spawn(async move {
    // The task receives a clone of the Arc, not the engine itself.
    let engine_clone = Arc::clone(&shared_engine);

    // When a transaction needs to be processed:
    // 1. Lock the mutex to get exclusive access.
    let mut engine_guard = engine_clone.lock().unwrap();
    // 2. Process the transaction. No other thread can interfere.
    engine_guard.process(record);
    // 3. The lock is automatically released here.
});
```
This pattern ensures that while thousands of connections are handled concurrently, the actual state modifications within the `PaymentEngine` are serialized, guaranteeing data consistency without requiring the engine itself to be internally aware of threads.
