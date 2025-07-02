use crate::error::EngineError;
use crate::models::{
    Account, InputRecord, OutputRecord, TransactionRecord, TransactionStatus, TransactionType,
};
use rust_decimal::Decimal;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::Write;

/// The core of the payment processing system.
/// It maintains the state of all client accounts and transactions.
pub struct PaymentEngine {
    /// Stores the state of each client account, keyed by client ID.
    accounts: HashMap<u16, Account>,
    /// Stores deposit transactions that can be disputed, keyed by transaction ID.
    transactions: HashMap<u32, TransactionRecord>,
}

impl PaymentEngine {
    /// Creates a new, empty payment engine.
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            transactions: HashMap::new(),
        }
    }

    /// Processes a single transaction record, updating the engine's state.
    /// Returns a specific error if the transaction is invalid.
    pub fn process(&mut self, record: InputRecord) -> Result<(), EngineError> {
        match record.transaction_type {
            TransactionType::Deposit => self.handle_deposit(record),
            TransactionType::Withdrawal => self.handle_withdrawal(record),
            TransactionType::Dispute => self.handle_dispute(record),
            TransactionType::Resolve => self.handle_resolve(record),
            TransactionType::Chargeback => self.handle_chargeback(record),
        }
    }

    /// Writes the final state of all accounts to a CSV writer.
    pub fn write_output<W: Write>(&self, wtr: &mut csv::Writer<W>) -> Result<(), csv::Error> {
        for (client_id, account) in self.accounts.iter() {
            let output_record = OutputRecord {
                client_id: *client_id,
                available: account.available,
                held: account.held,
                total: account.total(),
                locked: account.locked,
            };
            wtr.serialize(output_record)?;
        }
        Ok(())
    }

    // --- Private Handler Methods ---

    fn handle_deposit(&mut self, record: InputRecord) -> Result<(), EngineError> {
        let amount = record.amount.ok_or(EngineError::MissingAmount(record.tx_id))?;
        if amount <= Decimal::ZERO {
            return Err(EngineError::AmountNotPositive(record.tx_id));
        }

        if let Entry::Vacant(e) = self.transactions.entry(record.tx_id) {
            let account = self.accounts.entry(record.client_id).or_default();
            if account.locked {
                return Err(EngineError::AccountLocked(record.client_id));
            }
            account.deposit(amount);
            e.insert(TransactionRecord {
                amount,
                status: TransactionStatus::Normal,
            });
            Ok(())
        } else {
            Err(EngineError::DuplicateTransactionId(record.tx_id))
        }
    }

    fn handle_withdrawal(&mut self, record: InputRecord) -> Result<(), EngineError> {
        let amount = record.amount.ok_or(EngineError::MissingAmount(record.tx_id))?;
        if amount <= Decimal::ZERO {
            return Err(EngineError::AmountNotPositive(record.tx_id));
        }

        if let Some(account) = self.accounts.get_mut(&record.client_id) {
            if account.locked {
                return Err(EngineError::AccountLocked(record.client_id));
            }
            account
                .withdraw(amount)
                .map_err(|e| match e {
                    EngineError::InsufficientFunds(_, _) => EngineError::InsufficientFunds(record.client_id, amount),
                    _ => e,
                })?;
        }
        // Note: If account doesn't exist, withdrawal implicitly fails, which is valid.
        Ok(())
    }

    fn handle_dispute(&mut self, record: InputRecord) -> Result<(), EngineError> {
        let tx_id = record.tx_id;
        if let Some(tx) = self.transactions.get_mut(&tx_id) {
            if tx.status == TransactionStatus::Disputed {
                // Idempotent: if already disputed, do nothing.
                return Ok(());
            }
            if let Some(account) = self.accounts.get_mut(&record.client_id) {
                if account.locked {
                    return Err(EngineError::AccountLocked(record.client_id));
                }
                account.hold_for_dispute(tx.amount);
                tx.status = TransactionStatus::Disputed;
                Ok(())
            } else {
                // If the client account doesn't exist, this is an invalid dispute.
                Err(EngineError::TransactionNotFound(tx_id))
            }
        } else {
            Err(EngineError::TransactionNotFound(tx_id))
        }
    }

    fn handle_resolve(&mut self, record: InputRecord) -> Result<(), EngineError> {
        let tx_id = record.tx_id;
        if let Some(tx) = self.transactions.get_mut(&tx_id) {
            if tx.status != TransactionStatus::Disputed {
                return Err(EngineError::TransactionNotDisputed(tx_id));
            }
            if let Some(account) = self.accounts.get_mut(&record.client_id) {
                if account.locked {
                    return Err(EngineError::AccountLocked(record.client_id));
                }
                account.release_from_dispute(tx.amount);
                tx.status = TransactionStatus::Normal;
                Ok(())
            } else {
                // If the client account doesn't exist, this is an invalid resolve.
                Err(EngineError::TransactionNotFound(tx_id))
            }

        } else {
            Err(EngineError::TransactionNotFound(tx_id))
        }
    }

    fn handle_chargeback(&mut self, record: InputRecord) -> Result<(), EngineError> {
        let tx_id = record.tx_id;
        if let Some(tx) = self.transactions.get_mut(&tx_id) {
            if tx.status != TransactionStatus::Disputed {
                return Err(EngineError::TransactionNotDisputed(tx_id));
            }
            if let Some(account) = self.accounts.get_mut(&record.client_id) {
                // A chargeback proceeds even if the account is locked.
                // It finalizes the held funds removal and ensures the account is locked.
                account.chargeback(tx.amount);
                Ok(())
            } else {
                // If the client account doesn't exist, this is an invalid chargeback.
                Err(EngineError::TransactionNotFound(tx_id))
            }

        } else {
            Err(EngineError::TransactionNotFound(tx_id))
        }
    }
}

// --- Unit Tests ---
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::EngineError;
    use rust_decimal_macros::dec;

    fn process_record(engine: &mut PaymentEngine, record: InputRecord) -> Result<(), EngineError> {
        engine.process(record)
    }

    #[test]
    fn test_deposit_and_withdrawal() {
        let mut engine = PaymentEngine::new();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Deposit, client_id: 1, tx_id: 1, amount: Some(dec!(100.0)) }).unwrap();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Withdrawal, client_id: 1, tx_id: 2, amount: Some(dec!(30.0)) }).unwrap();
        let account = engine.accounts.get(&1).unwrap();
        assert_eq!(account.available, dec!(70.0));
        assert_eq!(account.total(), dec!(70.0));
    }

    #[test]
    fn test_insufficient_funds() {
        let mut engine = PaymentEngine::new();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Deposit, client_id: 1, tx_id: 1, amount: Some(dec!(20.0)) }).unwrap();
        let result = process_record(&mut engine, InputRecord { transaction_type: TransactionType::Withdrawal, client_id: 1, tx_id: 2, amount: Some(dec!(50.0)) });
        assert_eq!(result, Err(EngineError::InsufficientFunds(1, dec!(50.0))));
        let account = engine.accounts.get(&1).unwrap();
        assert_eq!(account.available, dec!(20.0));
    }

    #[test]
    fn test_full_dispute_resolve_cycle() {
        let mut engine = PaymentEngine::new();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Deposit, client_id: 1, tx_id: 1, amount: Some(dec!(100.0)) }).unwrap();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Dispute, client_id: 1, tx_id: 1, amount: None }).unwrap();
        let account_after_dispute = engine.accounts.get(&1).unwrap();
        assert_eq!(account_after_dispute.available, dec!(0));
        assert_eq!(account_after_dispute.held, dec!(100.0));
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Resolve, client_id: 1, tx_id: 1, amount: None }).unwrap();
        let account_after_resolve = engine.accounts.get(&1).unwrap();
        assert_eq!(account_after_resolve.available, dec!(100.0));
        assert_eq!(account_after_resolve.held, dec!(0));
    }

    #[test]
    fn test_full_chargeback_cycle() {
        let mut engine = PaymentEngine::new();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Deposit, client_id: 1, tx_id: 1, amount: Some(dec!(100.0)) }).unwrap();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Dispute, client_id: 1, tx_id: 1, amount: None }).unwrap();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Chargeback, client_id: 1, tx_id: 1, amount: None }).unwrap();
        let account = engine.accounts.get(&1).unwrap();
        assert_eq!(account.total(), dec!(0));
        assert!(account.locked);
    }

    #[test]
    fn test_tx_on_locked_account() {
        let mut engine = PaymentEngine::new();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Deposit, client_id: 1, tx_id: 1, amount: Some(dec!(100.0)) }).unwrap();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Dispute, client_id: 1, tx_id: 1, amount: None }).unwrap();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Chargeback, client_id: 1, tx_id: 1, amount: None }).unwrap();
        let result = process_record(&mut engine, InputRecord { transaction_type: TransactionType::Deposit, client_id: 1, tx_id: 2, amount: Some(dec!(50.0)) });
        assert_eq!(result, Err(EngineError::AccountLocked(1)));
    }

    #[test]
    fn test_error_on_resolving_undisputed_tx() {
        let mut engine = PaymentEngine::new();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Deposit, client_id: 1, tx_id: 1, amount: Some(dec!(100.0)) }).unwrap();
        let result = process_record(&mut engine, InputRecord { transaction_type: TransactionType::Resolve, client_id: 1, tx_id: 1, amount: None });
        assert_eq!(result, Err(EngineError::TransactionNotDisputed(1)));
    }

    #[test]
    fn test_error_on_duplicate_transaction_id() {
        let mut engine = PaymentEngine::new();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Deposit, client_id: 1, tx_id: 1, amount: Some(dec!(100.0)) }).unwrap();
        let result = process_record(&mut engine, InputRecord { transaction_type: TransactionType::Deposit, client_id: 2, tx_id: 1, amount: Some(dec!(50.0)) });
        assert_eq!(result, Err(EngineError::DuplicateTransactionId(1)));
    }

    #[test]
    fn test_dispute_non_existent_tx_is_ignored() {
        let mut engine = PaymentEngine::new();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Deposit, client_id: 1, tx_id: 1, amount: Some(dec!(100.0)) }).unwrap();
        let result = process_record(&mut engine, InputRecord { transaction_type: TransactionType::Dispute, client_id: 1, tx_id: 99, amount: None });
        assert_eq!(result, Err(EngineError::TransactionNotFound(99)));
        // Ensure original account is unchanged
        let account = engine.accounts.get(&1).unwrap();
        assert_eq!(account.available, dec!(100.0));
    }

    #[test]
    fn test_disputing_an_already_disputed_tx_is_idempotent() {
        let mut engine = PaymentEngine::new();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Deposit, client_id: 1, tx_id: 1, amount: Some(dec!(100.0)) }).unwrap();
        process_record(&mut engine, InputRecord { transaction_type: TransactionType::Dispute, client_id: 1, tx_id: 1, amount: None }).unwrap();
        // Second dispute should succeed with Ok(()) and not change state
        let result = process_record(&mut engine, InputRecord { transaction_type: TransactionType::Dispute, client_id: 1, tx_id: 1, amount: None });
        assert!(result.is_ok());
        // Check that state is still the same (funds are held, not held twice)
        let account = engine.accounts.get(&1).unwrap();
        assert_eq!(account.available, dec!(0));
        assert_eq!(account.held, dec!(100.0));
    }

    #[test]
    fn test_withdrawal_from_non_existent_client_is_ignored() {
        let mut engine = PaymentEngine::new();
        // No deposits for client 1
        let result = process_record(&mut engine, InputRecord { transaction_type: TransactionType::Withdrawal, client_id: 1, tx_id: 1, amount: Some(dec!(100.0)) });
        assert!(result.is_ok());
        // Ensure no account was created
        assert!(engine.accounts.get(&1).is_none());
    }
}