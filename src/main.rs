mod engine;
mod error;
mod models;

use engine::PaymentEngine;
use error::AppError;
use models::InputRecord;
use std::io;

fn main() -> Result<(), AppError> {
    // Get the input file path from the first command-line argument.
    let file_path = std::env::args()
        .nth(1)
        .ok_or(AppError::Usage("Usage: payment-engine <input_file.csv>".to_string()))?;

    // Initialize the payment engine.
    let mut engine = PaymentEngine::new();

    // Create a CSV reader. Trim whitespace to handle variations in input formatting.
    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .comment(Some(b'#'))// Added for testing transactions.csv
        .from_path(&file_path)?;

    // Process each record from the CSV.
    for result in rdr.deserialize::<InputRecord>() {
        match result {
            Ok(record) => {
                // Process the valid record. If an error occurs (e.g., insufficient funds),
                // print it to stderr and continue, as per the requirements.
                if let Err(e) = engine.process(record) {
                    eprintln!("Warning: {}", e);
                }
            }
            Err(e) => {
                // If a row is malformed, print an error to stderr and continue.
                eprintln!("Warning: Failed to parse a record, skipping. Error: {}", e);
            }
        }
    }

    // After processing all transactions, write the final account states to stdout.
    let mut wtr = csv::Writer::from_writer(io::stdout());
    engine.write_output(&mut wtr)?;
    wtr.flush()?;

    Ok(())
}