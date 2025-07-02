use crate::error::EngineError;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize, Serializer};

/// The type of transaction being processed.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

/// A single record from the input CSV file.
#[derive(Debug, Deserialize)]
pub struct InputRecord {
    #[serde(rename = "type")]
    pub transaction_type: TransactionType,
    #[serde(rename = "client")]
    pub client_id: u16,
    #[serde(rename = "tx")]
    pub tx_id: u32,
    pub amount: Option<Decimal>,
}

/// The state of a single client account. Fields are private to enforce state changes via methods.
#[derive(Debug, Default, PartialEq)]
pub struct Account {
    pub(crate) available: Decimal,
    pub(crate) held: Decimal,
    pub(crate) locked: bool,
}

impl Account {
    /// Deposits a given amount into the account.
    pub fn deposit(&mut self, amount: Decimal) {
        self.available += amount;
    }

    /// Withdraws a given amount from the account.
    pub fn withdraw(&mut self, amount: Decimal) -> Result<(), EngineError> {
        if self.available < amount {
            return Err(EngineError::InsufficientFunds(0, amount)); // Client ID filled by engine
        }
        self.available -= amount;
        Ok(())
    }

    /// Moves funds from 'available' to 'held' for a dispute.
    pub fn hold_for_dispute(&mut self, amount: Decimal) {
        self.available -= amount;
        self.held += amount;
    }

    /// Moves funds from 'held' back to 'available' for a resolution.
    pub fn release_from_dispute(&mut self, amount: Decimal) {
        self.held -= amount;
        self.available += amount;
    }

    /// Reverses a transaction by removing held funds and locks the account.
    pub fn chargeback(&mut self, amount: Decimal) {
        self.held -= amount;
        self.locked = true;
    }

    /// Calculates the total funds in the account (available + held).
    pub fn total(&self) -> Decimal {
        self.available + self.held
    }
}

/// A custom serialization function to format a Decimal to exactly four decimal places.
fn serialize_with_four_decimals<S>(value: &Decimal, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // The `round_dp` function ensures the value is mathematically correct to 4 places.
    // The format! macro then ensures the string representation has trailing zeros.
    let formatted_value = format!("{:.4}", value.round_dp(4));
    serializer.serialize_str(&formatted_value)
}

/// A record of a deposit transaction, stored for potential disputes.
/// Optimized to not store client_id, as it's redundant.
#[derive(Debug, Clone, Copy)]
pub struct TransactionRecord {
    pub amount: Decimal,
    pub status: TransactionStatus,
}

/// The status of a transaction, used to track the dispute lifecycle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransactionStatus {
    Normal,
    Disputed,
}

/// A single record for the output CSV file.
#[derive(Debug, Serialize)]
pub struct OutputRecord {
    #[serde(rename = "client")]
    pub client_id: u16,
    #[serde(serialize_with = "serialize_with_four_decimals")]
    pub available: Decimal,
    #[serde(serialize_with = "serialize_with_four_decimals")]
    pub held: Decimal,
    #[serde(serialize_with = "serialize_with_four_decimals")]
    pub total: Decimal,
    pub locked: bool,
}