# AI Tool Usage Declaration

I used Claude as a development assistant to accelerate certain aspects of this assignment.

---

## My Core Engineering Work

### 1. **Architecture & Design**
- Designed three-module separation: `main.rs` (CLI), `models.rs` (data), `engine.rs` (logic)
- Chose `rust_decimal` for financial precision (avoiding float arithmetic issues)
- Designed transaction log cleanup strategy for memory efficiency
- Made business rule decisions:
  - Disputes only on deposits (fraud reversal logic)
  - Idempotency via duplicate transaction rejection
  - Locked accounts block all new transactions
  - Negative available balance allowed during disputes

### 2. **Core Business Logic**
All transaction processing logic in `engine.rs`:
- `process_transactions()` streaming architecture
- Transaction handlers: `apply_deposit`, `apply_withdrawal`, `apply_dispute`, `apply_resolve`, `apply_chargeback`
- Validation rules (insufficient funds, locked accounts, negative amounts)
- Dispute state management with `HashSet`
- Transaction cleanup after resolution

### 3. **Test Strategy**
- Designed test-driven development approach
- Identified edge cases to cover (duplicate TXs, max values, locked states, idempotency)
- Defined test scenarios for complex flows (dispute chains, state transitions)
- Wrote integration tests and determined expected behaviors

### 4. **Data Modeling**
- Designed `OperationType` enum with custom deserializer
- Created `TransactionState` and `ClientBalance` structures
- Chose data structures (HashMap for O(1) lookups, HashSet for dispute tracking)

---

## Where I Used Claude as a Tool

### 1. **Test-Driven Development Assistance**

**My approach:**
- I practice TDD: write tests first, then implement features
- For simpler test cases, I had Claude generate boilerplate test functions

**What I did:**
- Designed all test scenarios (what to test)
- Wrote complex tests (dispute chains, integration flows) 
- Had Claude generate structure 

**Files affected:** `src/engine.rs` (tests module), `src/models.rs` (tests module)

---

### 2. **Test Data Generation**

**Claude's role:**
- Generated `examples/sample_input.csv` based on spec example
- Created `examples/dispute_sample_input.csv` for dispute chain testing
- Generated `examples/edge_cases_input.csv` for testing invalid operations (duplicate TXs, non-existent disputes, etc.)
- Created `examples/precision_and_lock_input.csv` for testing 4-decimal precision and locked account blocking
- Calculated expected output files for all scenarios

**My role:**
- Specified what scenarios to cover (basic transactions, dispute lifecycle, edge cases, precision requirements, locked account behavior)
- Verified correctness by running through my implementation
- Validated outputs matched my business logic and error handling
- Ensured spec requirements (4 decimal places, account freezing) were properly tested

**Files affected:** `examples/*.csv` (8 files total)

---

### 3. **Performance Test Optimization**

**Context:**
- I had written a performance test, but it pre-allocated 10M records
- Realized this didn't prove streaming (spec requirement)
- Needed to demonstrate true on-demand data generation

**My approach:**
- Prompted Claude to implement `std::io::Read` trait
- Made decision to use this vs. alternatives (temp file, etc.)

**What Claude provided:**
- `StreamingCsvGenerator` struct skeleton
- `Read` trait implementation with 8KB buffering

**What I did:**
- Reviewed the implementation thoroughly
- Modified test assertions and timeouts
- Added correctness validation checks
- Wrote documentation comments explaining the approach

**Files affected:** `src/engine.rs` lines 424-538 (~60 lines, but I understand every line)

---

### 4. **Documentation Review**

**My approach:**
- Wrote initial README with my thought process
- Asked Claude to review for clarity and completeness
- Claude suggested expanding the "Efficiency" section with more detail

---

### 5. **Syntax & Boilerplate Lookups**

- Serde deserialization syntax for case-insensitive enums
- Cargo.toml dependency version compatibility
- CSV crate API usage patterns
