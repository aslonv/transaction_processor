use anyhow::{Context, Result};
use csv::Reader;
use rust_decimal::Decimal;
use std::collections::{HashMap, HashSet};

use crate::models::{ClientBalance, OperationRecord, OperationType, TransactionState};

pub fn process_transactions(
    rdr: &mut Reader<impl std::io::Read>,
) -> Result<HashMap<u16, ClientBalance>> {
    let mut client_balances: HashMap<u16, ClientBalance> = HashMap::new();
    let mut transaction_log: HashMap<u32, TransactionState> = HashMap::new();
    let mut dispute_tracker: HashSet<u32> = HashSet::new();

    for result in rdr.deserialize() {
        let record: OperationRecord = result.context("Failed to deserialize record")?;

        let balance = client_balances
            .entry(record.client)
            .or_insert_with(ClientBalance::new);

        match record.r#type {
            OperationType::Deposit => apply_deposit(
                &mut transaction_log,
                balance,
                record.tx,
                record.client,
                record.amount,
            ),
            OperationType::Withdrawal => apply_withdrawal(
                balance,
                record.tx,
                record.client,
                record.amount,
                &mut transaction_log,
            ),
            OperationType::Dispute => apply_dispute(
                balance,
                record.tx,
                record.client,
                &transaction_log,
                &mut dispute_tracker,
            ),
            OperationType::Resolve => {
                apply_resolve(
                    balance,
                    record.tx,
                    record.client,
                    &transaction_log,
                    &mut dispute_tracker,
                )?;
                cleanup_transaction(&mut transaction_log, &dispute_tracker, record.tx);
            }
            OperationType::Chargeback => {
                apply_chargeback(
                    balance,
                    record.tx,
                    record.client,
                    &transaction_log,
                    &mut dispute_tracker,
                )?;
                cleanup_transaction(&mut transaction_log, &dispute_tracker, record.tx);
            }
        };
    }

    Ok(client_balances)
}

fn apply_deposit(
    transaction_log: &mut HashMap<u32, TransactionState>,
    balance: &mut ClientBalance,
    tx: u32,
    client: u16,
    amount: Option<Decimal>,
) {
    if let Some(amt) = amount {
        if amt > Decimal::ZERO && !balance.locked && !transaction_log.contains_key(&tx) {
            balance.available += amt;
            transaction_log.insert(
                tx,
                TransactionState {
                    client,
                    amount: amt,
                    is_deposit: true,
                },
            );
        }
    }
}

fn apply_withdrawal(
    balance: &mut ClientBalance,
    tx: u32,
    client: u16,
    amount: Option<Decimal>,
    transaction_log: &mut HashMap<u32, TransactionState>,
) {
    if let Some(amt) = amount {
        if amt > Decimal::ZERO
            && !balance.locked
            && balance.available >= amt
            && !transaction_log.contains_key(&tx)
        {
            balance.available -= amt;
            transaction_log.insert(
                tx,
                TransactionState {
                    client,
                    amount: amt,
                    is_deposit: false,
                },
            );
        }
    }
}

fn apply_dispute(
    balance: &mut ClientBalance,
    tx: u32,
    client: u16,
    transaction_log: &HashMap<u32, TransactionState>,
    dispute_tracker: &mut HashSet<u32>,
) {
    if let Some(state) = transaction_log.get(&tx) {
        if state.client == client && state.is_deposit && dispute_tracker.insert(tx) {
            let amt = state.amount;
            balance.available -= amt;
            balance.held += amt;
        }
    }
}

fn apply_resolve(
    balance: &mut ClientBalance,
    tx: u32,
    client: u16,
    transaction_log: &HashMap<u32, TransactionState>,
    dispute_tracker: &mut HashSet<u32>,
) -> Result<()> {
    if let Some(state) = transaction_log.get(&tx) {
        if state.client == client && dispute_tracker.remove(&tx) {
            let amt = state.amount;
            balance.available += amt;
            balance.held -= amt;
        }
    }
    Ok(())
}

fn apply_chargeback(
    balance: &mut ClientBalance,
    tx: u32,
    client: u16,
    transaction_log: &HashMap<u32, TransactionState>,
    dispute_tracker: &mut HashSet<u32>,
) -> Result<()> {
    if let Some(state) = transaction_log.get(&tx) {
        if state.client == client && dispute_tracker.remove(&tx) {
            let amt = state.amount;
            balance.held -= amt;
            balance.locked = true;
        }
    }
    Ok(())
}

fn cleanup_transaction(
    transaction_log: &mut HashMap<u32, TransactionState>,
    dispute_tracker: &HashSet<u32>,
    tx: u32,
) {
    if !dispute_tracker.contains(&tx) {
        transaction_log.remove(&tx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::*;
    use csv::ReaderBuilder;
    use rand::Rng;
    use rust_decimal_macros::dec;
    use std::fs::File;
    use std::io::{Cursor, Write};
    use tempfile::NamedTempFile;

    fn create_balance() -> ClientBalance {
        ClientBalance::new()
    }

    #[test]
    fn test_apply_deposit() {
        let mut log = HashMap::new();
        let mut balance = create_balance();
        apply_deposit(&mut log, &mut balance, 1, 1, Some(dec!(10.1234)));
        assert_eq!(balance.available, dec!(10.1234));
        assert_eq!(balance.held, dec!(0));
        assert!(log.contains_key(&1));
    }

    #[test]
    fn test_apply_withdrawal_success() {
        let mut log = HashMap::new();
        let mut balance = create_balance();
        balance.available = dec!(5.0);
        apply_withdrawal(&mut balance, 1, 1, Some(dec!(3.0)), &mut log);
        assert_eq!(balance.available, dec!(2.0));
        assert!(log.contains_key(&1));
    }

    #[test]
    fn test_apply_withdrawal_fail_insufficient() {
        let mut log = HashMap::new();
        let mut balance = create_balance();
        balance.available = dec!(1.0);
        apply_withdrawal(&mut balance, 1, 1, Some(dec!(2.0)), &mut log);
        assert_eq!(balance.available, dec!(1.0));
        assert!(!log.contains_key(&1));
    }

    #[test]
    fn test_apply_withdrawal_fail_locked() {
        let mut log = HashMap::new();
        let mut balance = create_balance();
        balance.available = dec!(5.0);
        balance.locked = true;
        apply_withdrawal(&mut balance, 1, 1, Some(dec!(3.0)), &mut log);
        assert_eq!(balance.available, dec!(5.0));
        assert!(!log.contains_key(&1));
    }

    #[test]
    fn test_apply_dispute() {
        let mut log = HashMap::new();
        let mut tracker = HashSet::new();
        let mut balance = create_balance();
        log.insert(
            1,
            TransactionState {
                client: 1,
                amount: dec!(10.0),
                is_deposit: true,
            },
        );
        apply_dispute(&mut balance, 1, 1, &log, &mut tracker);
        assert_eq!(balance.available, dec!(-10.0));
        assert_eq!(balance.held, dec!(10.0));
        assert!(tracker.contains(&1));
    }

    #[test]
    fn test_apply_dispute_ignore_non_deposit() {
        let mut log = HashMap::new();
        let mut tracker = HashSet::new();
        let mut balance = create_balance();
        log.insert(
            1,
            TransactionState {
                client: 1,
                amount: dec!(10.0),
                is_deposit: false,
            },
        );
        apply_dispute(&mut balance, 1, 1, &log, &mut tracker);
        assert_eq!(balance.available, dec!(0));
        assert_eq!(balance.held, dec!(0));
        assert!(!tracker.contains(&1));
    }

    #[test]
    fn test_apply_resolve() -> Result<()> {
        let mut log = HashMap::new();
        let mut tracker = HashSet::new();
        let mut balance = create_balance();
        log.insert(
            1,
            TransactionState {
                client: 1,
                amount: dec!(10.0),
                is_deposit: true,
            },
        );
        tracker.insert(1);
        balance.available = dec!(-10.0);
        balance.held = dec!(10.0);
        apply_resolve(&mut balance, 1, 1, &log, &mut tracker)?;
        assert_eq!(balance.available, dec!(0));
        assert_eq!(balance.held, dec!(0));
        assert!(!tracker.contains(&1));
        Ok(())
    }

    #[test]
    fn test_apply_chargeback() -> Result<()> {
        let mut log = HashMap::new();
        let mut tracker = HashSet::new();
        let mut balance = create_balance();
        log.insert(
            1,
            TransactionState {
                client: 1,
                amount: dec!(10.0),
                is_deposit: true,
            },
        );
        tracker.insert(1);
        balance.held = dec!(10.0);
        apply_chargeback(&mut balance, 1, 1, &log, &mut tracker)?;
        assert_eq!(balance.available, dec!(0));
        assert_eq!(balance.held, dec!(0));
        assert!(balance.locked);
        assert!(!tracker.contains(&1));
        Ok(())
    }

    #[test]
    fn test_idempotency_duplicate_deposit() {
        let mut log = HashMap::new();
        let mut balance = create_balance();
        apply_deposit(&mut log, &mut balance, 1, 1, Some(dec!(10.0)));
        apply_deposit(&mut log, &mut balance, 1, 1, Some(dec!(10.0))); // Duplicate ignored
        assert_eq!(balance.available, dec!(10.0));
    }

    #[test]
    fn test_negative_zero_amount_skip() {
        let mut log = HashMap::new();
        let mut balance = create_balance();
        apply_deposit(&mut log, &mut balance, 1, 1, Some(dec!(0)));
        apply_deposit(&mut log, &mut balance, 2, 1, Some(dec!(-1.0)));
        assert_eq!(balance.available, dec!(0));
        assert!(!log.contains_key(&1));
        assert!(!log.contains_key(&2));
    }

    #[test]
    fn test_post_lock_block() {
        let mut log = HashMap::new();
        let mut balance = create_balance();
        balance.locked = true;
        apply_deposit(&mut log, &mut balance, 1, 1, Some(dec!(10.0)));
        assert_eq!(balance.available, dec!(0));
    }

    #[test]
    fn test_max_values() {
        let mut log = HashMap::new();
        let mut balance = create_balance();
        apply_deposit(
            &mut log,
            &mut balance,
            u32::MAX,
            u16::MAX,
            Some(dec!(10000000000.9999)),
        );
        assert_eq!(balance.available, dec!(10000000000.9999));
        assert!(log.contains_key(&u32::MAX));
    }

    #[test]
    fn test_cleanup_after_resolve() -> Result<()> {
        let mut log = HashMap::new();
        let mut tracker = HashSet::new();
        let mut balance = create_balance();
        log.insert(
            1,
            TransactionState {
                client: 1,
                amount: dec!(10.0),
                is_deposit: true,
            },
        );
        tracker.insert(1);
        apply_resolve(&mut balance, 1, 1, &log, &mut tracker)?;
        cleanup_transaction(&mut log, &tracker, 1);
        assert!(!log.contains_key(&1));
        Ok(())
    }

    #[test]
    fn integration_test_sample() -> Result<()> {
        let data = "type,client,tx,amount\ndeposit,1,1,1.0\ndeposit,2,2,2.0\ndeposit,1,3,2.0\nwithdrawal,1,4,1.5\nwithdrawal,2,5,3.0";
        let mut rdr = ReaderBuilder::new()
            .flexible(true)
            .from_reader(Cursor::new(data));
        let balances = process_transactions(&mut rdr)?;
        assert_eq!(balances.len(), 2);
        let b1 = balances.get(&1).unwrap();
        assert_eq!(b1.available, dec!(1.5));
        assert_eq!(b1.held, dec!(0.0));
        assert!(!b1.locked);
        let b2 = balances.get(&2).unwrap();
        assert_eq!(b2.available, dec!(2.0)); // Withdrawal fails due to insufficient
        assert_eq!(b2.held, dec!(0.0));
        assert!(!b2.locked);
        Ok(())
    }

    #[test]
    fn integration_test_dispute_chain() -> Result<()> {
        let mut file = NamedTempFile::new()?;
        let _ = writeln!(file, "type,client,tx,amount\ndeposit,1,1,10.0\ndispute,1,1\nresolve,1,1\ndeposit,1,2,5.0\ndispute,1,2\nchargeback,1,2");
        let file_path = file.path().to_str().unwrap().to_string();
        let file = File::open(file_path)?;
        let mut rdr = ReaderBuilder::new().flexible(true).from_reader(file);
        let balances = process_transactions(&mut rdr)?;
        let b = balances.get(&1).unwrap();
        assert_eq!(b.available, dec!(10.0));
        assert_eq!(b.held, dec!(0.0));
        assert!(b.locked);
        Ok(())
    }

    /// Mocks CSV generator that streams transaction data without pre-allocating.
    /// It will handle large datasets via streaming.
    struct StreamingCsvGenerator {
        current_tx: usize,
        total_txs: usize,
        num_clients: u16,
        rng: rand::rngs::ThreadRng,
        buffer: Vec<u8>,
        buffer_pos: usize,
    }

    impl StreamingCsvGenerator {
        fn new(total_txs: usize, num_clients: u16) -> Self {
            let header = b"type,client,tx,amount\n".to_vec();
            Self {
                current_tx: 0,
                total_txs,
                num_clients,
                rng: rand::thread_rng(),
                buffer: header,
                buffer_pos: 0,
            }
        }

        fn generate_next_line(&mut self) {
            if self.current_tx < self.total_txs {
                let client = self.rng.gen_range(1..=self.num_clients);
                let line = format!("deposit,{},{},1.0\n", client, self.current_tx);
                self.buffer.extend_from_slice(line.as_bytes());
                self.current_tx += 1;
            }
        }
    }

    impl std::io::Read for StreamingCsvGenerator {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            // If buffer is exhausted, generate more lines
            if self.buffer_pos >= self.buffer.len() {
                if self.current_tx >= self.total_txs {
                    return Ok(0); 
                }
                self.buffer.clear();
                self.buffer_pos = 0;

                // Generate a batch of lines to fill buffer (up to 8KB for efficiency)
                while self.buffer.len() < 8192 && self.current_tx < self.total_txs {
                    self.generate_next_line();
                }
            }

            // Copy from internal buffer to output buffer
            let remaining = self.buffer.len() - self.buffer_pos;
            let to_copy = remaining.min(buf.len());
            buf[..to_copy]
                .copy_from_slice(&self.buffer[self.buffer_pos..self.buffer_pos + to_copy]);
            self.buffer_pos += to_copy;

            Ok(to_copy)
        }
    }

    #[test]
    #[ignore]
    fn perf_test_large_dataset() -> Result<()> {
        let num_txs = 10_000_000;
        let num_clients = 1000u16;

        // Create streaming generator (no pre-allocation of transactions)
        let generator = StreamingCsvGenerator::new(num_txs, num_clients);
        let mut rdr = ReaderBuilder::new()
            .flexible(true)
            .from_reader(generator);

        // Measure processing time
        let start = std::time::Instant::now();
        let client_balances = process_transactions(&mut rdr)?;
        let duration = start.elapsed().as_secs_f64();

        // Estimate memory (only stores client balances + transaction log for disputes)
        // In this test, no disputes occur, so transaction_log holds all deposits
        let mem_est = (num_txs * std::mem::size_of::<(u32, TransactionState)>())
            + (client_balances.len() * std::mem::size_of::<(u16, ClientBalance)>());
        let mem_mb = mem_est as f64 / 1_048_576.0;

        println!(
            "Streamed {} txs in {:.2}s ({:.0} tx/sec), {} clients, est. mem {:.2}MB",
            num_txs,
            duration,
            num_txs as f64 / duration,
            client_balances.len(),
            mem_mb
        );

        if cfg!(debug_assertions) {
            assert!(duration < 60.0, "Debug mode too slow: {:.2}s", duration);
        } else {
            assert!(
                duration < 20.0,
                "Release mode too slow: {:.2}s",
                duration
            );
        }
        assert!(mem_mb < 500.0, "Memory usage too high: {:.2}MB", mem_mb);

        assert_eq!(client_balances.len(), num_clients as usize);
        for balance in client_balances.values() {
            assert!(balance.available > dec!(0));
            assert_eq!(balance.held, dec!(0));
            assert!(!balance.locked);
        }

        Ok(())
    }
}
