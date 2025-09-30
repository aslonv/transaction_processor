# Payments Engine

A streaming transaction processor that handles deposits, withdrawals, disputes, and chargebacks. Built with Rust for safety, performance, and correctness in financial operations.

## Build and Test

```bash
# Development build
cargo build
cargo test

# Release build (optimized)
cargo build --release
cargo test --release

# Run with example data
cargo run -- examples/sample_input.csv > output.csv

# Performance test (10M transactions)
cargo test --release perf_test_large_dataset -- --ignored --nocapture
```

---

## Architecture Decisions

### Module Separation

I structured the codebase into three distinct modules:

**`main.rs`** handles the CLI interface and I/O orchestration. Its sole responsibility is reading the CSV, delegating processing to the engine, and formatting output. This separation means the business logic can be tested independently of file handling, and alternative input sources (database, message queue, HTTP) could be added without touching the core engine.

**`models.rs`** contains all data structures with zero business logic. I used Rust's type system defensively here - `OperationType` is an enum rather than strings to make invalid states unrepresentable. The custom deserializer handles case-insensitive input ("DEPOSIT", "Deposit", "deposit") since real-world CSV data is rarely clean. Critically, all amounts use `rust_decimal` rather than `f64` to avoid floating-point precision errors that plague financial calculations.

**`engine.rs`** implements the transaction processing logic. Each operation type (deposit, withdrawal, dispute, etc.) gets its own function with clear validation rules. This modularity makes the code easier to reason about and test - each function has a single responsibility and explicit pre/post-conditions.

---

## Key Implementation Choices

### Why rust_decimal Over f64?

Financial calculations with floating-point numbers are dangerous. The classic `0.1 + 0.2 != 0.3` problem isn't academic - it means client balances can drift over time. `rust_decimal` uses fixed-point arithmetic with 28-29 significant digits, guaranteeing exact decimal calculations. 

### Streaming vs Batch Processing

Each transaction is deserialized, processed, and dropped - we never hold the entire file in memory. This means the system can handle files larger than available RAM, which is critical when the spec mentions transaction IDs are valid u32 values (potentially 4.2 billion transactions).

To prove this works, I implemented the performance test with a custom `Read` trait that generates 10 million transactions on-the-fly without pre-allocation. This demonstrates the engine streaming data rather than batch-loading it.

### Transaction Log and Memory Management

A naive implementation would store every transaction permanently, leading to O(all transactions) memory usage. Instead, I maintain a transaction log only for transactions that might be disputed. When a dispute is resolved or charged back, the transaction is cleaned up via `cleanup_transaction()`. This reduces memory overhead to O(currently disputed transactions), which is typically 1-2 orders of magnitude smaller.

The trade-off is we can't retrieve historical transaction details after cleanup, but the spec only requires current account balances. 

For production, I would add database backing for full transaction history while keeping the in-memory log for performance.

### Idempotency and Duplicate Transactions

Real-world systems always face duplicate transactions (network retries, upstream errors, etc.). I chose to reject duplicate transaction IDs globally - if a transaction ID has been processed, subsequent attempts are silently ignored. This prevents double-spend attacks and makes the system more robust to messy input data.

---

## Business Logic Decisions

### Disputes Only on Deposits

Withdrawals are intentional client actions that have already left the system - disputing them doesn't make sense in this threat model. Therefore, I only allow disputes on deposits (enforced via `state.is_deposit` check).

An alternative interpretation would allow disputing withdrawals (e.g., unauthorized transactions), but the spec's emphasis on deposit fraud suggests this isn't the intent.

### Negative Available Balances During Disputes

The most subtle business logic decision: should we allow `available` to go negative when a dispute is raised? I assumed the following scenario:

1. Client deposits $100 (available: $100)
2. Client withdraws $80 (available: $20)
3. Original $100 deposit is disputed (available: $20 - $100 = **-$80**, held: $100)

The spec says "available funds should decrease by the amount disputed" without specifying a minimum balance check. I chose to allow negative available because:

- **The spec doesn't forbid it** - it says "decrease", not "decrease if sufficient funds exist"
- **It handles the fraud scenario correctly** - the client withdrew more than the "real" money, so they can owe the platform
- **Total remains correct** - the invariant `total = available + held` is maintained

The alternative (rejecting disputes on insufficient available) would let fraudsters keep withdrawn funds, defeating the purpose of disputes.

### Account Locking After Chargeback

When a chargeback occurs, the account is immediately frozen via `balance.locked = true`. I chose to block *all* operations (deposits, withdrawals, even new disputes) on locked accounts. In production, there would be an unlock mechanism, but for this system, permanent freezing after confirmed fraud would be the safest choice.

---

## Error Handling Strategy

I used `anyhow::Result` for application errors with `.context()` annotations to provide clear error messages. Invalid operations (disputes on non-existent transactions, withdrawals with insufficient funds, etc.) are silently ignored rather than returning errors.

---

## Testing Approach

I wrote tests at three levels:

**Unit tests** cover individual functions with known inputs and outputs. Each transaction handler gets tests for success cases (deposit works), failure cases (withdrawal with insufficient funds), and edge cases (negative amounts, locked accounts). The test for max u16/u32 values ensures we don't have overflow issues at type boundaries.

**Integration tests** verify end-to-end workflows with realistic transaction sequences. The dispute chain test (deposit → dispute → resolve, then deposit → dispute → chargeback) ensures state transitions work correctly across multiple operations.

The performance test : custom `Read` trait generates data on-the-fly. The program doesn't need to pre-load transactions (with respect to streaming large datasets).

---

## Assumptions and Ambiguities

The spec left some details ambiguous. Here are the assumptions I made and why:

**Transaction IDs are globally unique** - The spec says they're "valid u32 values" but doesn't specify scope. I assume global uniqueness because reusing IDs across clients would complicate dispute tracking and make the system fragile.

**Whitespace is allowed** - Real CSV files often have inconsistent spacing. I configured the CSV reader with `.trim(csv::Trim::All)` to handle "deposit, 1, 1, 1.0" and "deposit,1,1,1.0" identically.

**Transactions are chronologically ordered in the file** - The spec explicitly states this, so I don't need to sort by timestamp or handle out-of-order transactions.

**Precision is always 4 decimals** - The spec shows examples with varying decimal places but then says "should output values with the same level of precision" (4 decimals). I chose to always output 4 decimals for consistency, which is standard in financial systems.

---

## Performance Characteristics

### Measured Performance
- **Throughput**: 690k transactions/sec (release mode on test machine)
- **Memory usage**: 229MB for 10M transactions with 1000 clients
- **Scalability**: Linear time complexity O(n), sublinear space complexity O(clients + disputed_txs)

### Data Structure Choices

I used `HashMap<u16, ClientBalance>` for client accounts and `HashMap<u32, TransactionState>` for transaction tracking. HashMaps provide O(1) average-case lookups, which is critical when processing millions of transactions. The alternative (BTreeMap) would give O(log n) lookups, and while we'd get sorted iteration for free, it's not worth the performance cost since we sort client IDs once at output time.

For dispute tracking, I used `HashSet<u32>` because we only need to know if a transaction is currently disputed. The `insert()` and `remove()` operations are O(1), and the HashSet naturally prevents duplicate disputes.

---

## Production Considerations

This implementation is suitable for a synchronous, single-threaded processing pipeline. For production deployment at scale, I'd consider:

**Concurrency**: Wrapping the engine in async handlers (tokio) to process multiple streams concurrently. Since disputes reference past transactions, either per-client engine instances (client-level parallelism) or a shared transaction log with Arc<RwLock<>> (global parallelism with synchronization overhead) would be needed.

**Persistence**: Addding database backing for transaction history. The current in-memory transaction log is cleaned up after disputes resolve.

**Monitoring**: Instrument with metrics (transaction counts, error rates, processing latency) and structured logging. 

**Validation**: Adding deeper input validation - maybe check for suspiciously large amounts, rate limit disputes per client, detect patterns indicative of fraud. This implementation trusts the input data, which is fine for a controlled test but I think it would be risky in production.
