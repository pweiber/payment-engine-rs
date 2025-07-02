use thiserror::Error;

/// Defines the application-level errors that can occur.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Usage(String),
    #[error(transparent)]
    Csv(#[from] csv::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Defines all possible logical errors within the payment engine.
#[derive(Debug, Error, PartialEq)]
pub enum EngineError {
    #[error("Account {0} is locked")]
    AccountLocked(u16),
    #[error("Transaction {0} not found or is not a disputable deposit")]
    TransactionNotFound(u32),
    #[error("Transaction {0} is not currently under dispute")]
    TransactionNotDisputed(u32),
    #[error("Insufficient funds for client {0} to withdraw {1}")]
    InsufficientFunds(u16, rust_decimal::Decimal),
    #[error("Duplicate transaction ID {0}. Transaction ignored.")]
    DuplicateTransactionId(u32),
    #[error("Deposit or withdrawal for tx {0} must have a positive amount")]
    AmountNotPositive(u32),
    #[error("Deposit or withdrawal for tx {0} is missing an amount")]
    MissingAmount(u32),
}