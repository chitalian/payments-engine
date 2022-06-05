use csv::{self, StringRecord};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    num::{ParseFloatError, ParseIntError},
};

type Result<T> = std::result::Result<T, PaymentsEngineError>;
type FloatResult<T> = std::result::Result<T, ParseFloatError>;

#[derive(Debug)]
struct PaymentsEngineError(String);

impl From<String> for PaymentsEngineError {
    fn from(s: String) -> Self {
        PaymentsEngineError(s)
    }
}
impl From<&str> for PaymentsEngineError {
    fn from(s: &str) -> Self {
        PaymentsEngineError(s.to_string())
    }
}
#[derive(Debug, PartialEq)]
struct Transaction {
    transaction_type: TransactionType,
    client_id: u16,
    txn_id: u32,
    amount: Option<f64>,
}

#[derive(Debug, PartialEq)]
/// This is the type of transaction that is representative of a
/// single row within the CSV file.
enum TransactionType {
    /// A deposit is a credit to the client's account.
    /// Meaning this should increase the client's balance.
    Deposit,
    /// A withdrawal is a debit to the client's account.
    /// Meaning this should decrease the client's balance.
    Withdrawal,
    /// When a client claims the transaction was erroneous,
    /// and should be flagged to be reverted.
    /// This will put on hold the client's funds by the amount
    /// of the referenced transaction.
    Dispute,
    /// This happens when a client has a dispute and the dispute
    /// is resolved (or fails), meaning the funds should be returned to the
    /// client's account. And no transactions are reverted.
    /// The transaction must refer to a disputed transaction,
    /// and not the dispute transaction itself. If the transaction
    /// is not current disputed, we abort the transaction, and ignore it.
    Resolve,
    /// This happens when a client has a dispute and the dispute
    /// is successful
    ///
    /// The transaction must refer to a disputed transaction,
    /// and not the dispute transaction itself. If the transaction
    /// is not current disputed, we abort the transaction, and ignore it.
    ChargeBack,
}

/// Gets the passed in file name from
fn get_file_name_from_args() -> Result<String> {
    std::env::args()
        .nth(1)
        .ok_or("Must contain at least one argument".into())
}

/// Opens a csv and returns a reader
fn open_file_read_csv(filename: String) -> Result<csv::Reader<File>> {
    let file = File::open(filename).map_err(|x| format!("error code: {}", x))?;
    Ok(csv::Reader::from_reader(file))
}
impl From<csv::Error> for PaymentsEngineError {
    fn from(err: csv::Error) -> Self {
        PaymentsEngineError(format!("{}", err))
    }
}

impl From<ParseIntError> for PaymentsEngineError {
    fn from(err: ParseIntError) -> Self {
        PaymentsEngineError(format!("{}", err))
    }
}

impl From<ParseFloatError> for PaymentsEngineError {
    fn from(err: ParseFloatError) -> Self {
        PaymentsEngineError(format!("{}", err))
    }
}

impl TryFrom<&StringRecord> for Transaction {
    type Error = PaymentsEngineError;
    fn try_from(record: &StringRecord) -> Result<Self> {
        Ok(Transaction {
            transaction_type: record.try_into()?,
            client_id: record[1].replace(" ", "").parse::<u16>()?,
            txn_id: record[2].replace(" ", "").parse::<u32>()?,
            amount: record
                .get(3)
                .as_ref()
                .map_or::<FloatResult<_>, _>(Ok(None), |x| match x.replace(" ", "").as_str() {
                    "" => Ok(None),
                    x => Ok(Some(x.parse::<f64>()?)),
                })?,
        })
    }
}

impl TryFrom<&StringRecord> for TransactionType {
    type Error = PaymentsEngineError;
    fn try_from(record: &StringRecord) -> Result<Self> {
        Ok(match record.get(0) {
            Some("deposit") => TransactionType::Deposit,
            Some("withdrawal") => TransactionType::Withdrawal,
            Some("dispute") => TransactionType::Dispute,
            Some("resolve") => TransactionType::Resolve,
            Some("chargeback") => TransactionType::ChargeBack,
            _ => panic!("Unknown transaction type"),
        })
    }
}

#[derive(Default, Debug, PartialEq)]
/// This is the main data structure that we will use to store
/// all of the transactions.
struct Database {
    transactions: HashMap<u32, Transaction>,
    clients: HashMap<u16, Client>,
}

#[derive(Debug, PartialEq, Default)]
/// This struct represents the state of a single client's account.
struct Client {
    /// The client's available balance
    available: f64,
    /// The client's held balance if there was a dispute
    held: f64,
    /// Is the client's account is locked from a charge back
    locked: bool,
    /// Disputed transactions
    disputed: HashSet<u32>,
}

/// Handles a single transaction and updates the database accordingly.
fn handle_transaction(db: &mut Database, txn: Transaction) -> Result<()> {
    let client = db.clients.entry(txn.client_id).or_insert(Client::default());
    if client.locked {
        eprintln!(
            "Client {} is locked, aborting transaction {}",
            txn.client_id, txn.txn_id
        );
        return Ok(());
    }
    match (
        &txn.transaction_type,
        db.transactions.get(&txn.txn_id),
        txn.amount,
    ) {
        (TransactionType::Deposit, _, Some(amount)) => {
            client.available += amount;
            db.transactions.insert(txn.txn_id, txn);
        }
        (TransactionType::Withdrawal, _, Some(amount)) => {
            if client.available - amount < 0.0 {
                eprintln!("Client {} has insufficient funds", txn.client_id);
            } else {
                client.available -= amount;
            }
            db.transactions.insert(txn.txn_id, txn);
        }
        (
            TransactionType::Dispute,
            Some(Transaction {
                client_id,
                amount: Some(amount),
                txn_id,
                ..
            }),
            ..,
        ) => {
            if *client_id != txn.client_id {
                eprintln!(
                    "Client {} attempted to dispute transaction {}. Which was not it's transaction",
                    txn.client_id, txn.txn_id
                );
            } else {
                client.held += amount;
                client.available -= amount;
                client.disputed.insert(*txn_id);
            }
        }
        (
            TransactionType::Resolve,
            Some(Transaction {
                client_id,
                amount: Some(amount),
                txn_id,
                ..
            }),
            ..,
        ) => {
            if *client_id != txn.client_id {
                eprintln!(
                    "Client {} attempted to resolve transaction {}. Which was not it's transaction",
                    txn.client_id, txn.txn_id
                );
            } else {
                if client.disputed.contains(&txn_id) {
                    client.available += dbg!(amount);
                    client.held -= amount;
                } else {
                    eprintln!(
                        "Client {} attempted to resolve transaction {}. Which was not disputed",
                        txn.client_id, txn.txn_id
                    );
                }
            }
        }
        (
            TransactionType::ChargeBack,
            Some(Transaction {
                client_id,
                amount: Some(amount),
                txn_id,
                ..
            }),
            ..,
        ) => {
            if *client_id != txn.client_id {
                eprintln!(
                    "Client {} attempted to chargeback transaction {}. Which was not it's transaction",
                    txn.client_id, txn.txn_id
                );
            } else {
                if client.disputed.contains(&txn_id) {
                    client.held -= amount;
                    client.locked = true;
                } else {
                    eprintln!(
                        "Client {} attempted to chargeback transaction {}. Which was not disputed",
                        txn.client_id, txn.txn_id
                    );
                }
            }
        }
        _ => eprintln!("Unknown transaction type"),
    }
    Ok(())
}

/// Loads in the database with the given csv file.
/// This is designed in such a way that a Reader is inputted
/// and a possibly shared db can be used across multiple threads.
/// This is written to easily allow synchronization oh parsing data
/// and loading into a database.
fn run_engine(mut reader: csv::Reader<File>, mut db: &mut Database) -> Result<()> {
    for record in reader.records() {
        let txn = Transaction::try_from(&record?)?;
        handle_transaction(&mut db, txn)?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let filename = get_file_name_from_args()?;
    let reader = open_file_read_csv(filename)?;
    let mut db = Database::default();

    run_engine(reader, &mut db)?;
    println!(
        "{:>7}, {:>12}, {:>12}, {:>12}, {:>12}",
        "client", "available", "held", "total", "locked"
    );
    db.clients.iter().for_each(|(client_id, client)| {
        println!(
            "{:>7}, {:>12.4}, {:>12.4}, {:>12.4}, {:>12}",
            client_id,
            client.available,
            client.held,
            client.available + client.held,
            client.locked
        );
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_test_read_example_input() -> Result<()> {
        let reader = open_file_read_csv("test-files/example_input.csv".to_string())?;
        let mut db = Database::default();
        run_engine(reader, &mut db)?;
        assert_eq!(db.transactions.len(), 5);
        assert_eq!(db.clients.len(), 2);
        assert_eq!(db.clients[&1].available, 1.5);
        assert_eq!(db.clients[&2].available, 2.0);
        println!("{:?}", db);
        Ok(())
    }

    #[test]
    /// Tests this case stated in the problem statement.
    /// > Likewise, transaction IDs (tx) are globally unique, though are also not guaranteed to be ordered.
    fn order_does_not_matter() -> Result<()> {
        let reader_0 = open_file_read_csv("test-files/example_input_out_of_order.csv".to_string())?;
        let reader_1 = open_file_read_csv("test-files/example_input.csv".to_string())?;
        let mut db_0 = Database::default();
        let mut db_1 = Database::default();
        run_engine(reader_0, &mut db_0)?;
        run_engine(reader_1, &mut db_1)?;
        assert_eq!(db_0.clients, db_1.clients);
        Ok(())
    }

    #[test]
    /// Dispute a deposit transaction.
    fn test_dispute_deposit() -> Result<()> {
        let reader = open_file_read_csv("test-files/dispute_deposit.csv".to_string())?;
        let mut db = Database::default();
        run_engine(reader, &mut db)?;
        assert_eq!(db.transactions.len(), 2);
        assert_eq!(db.clients.len(), 1);
        assert_eq!(db.clients[&1].available, 2.0);
        assert_eq!(db.clients[&1].held, 1.0);
        Ok(())
    }
    #[test]
    fn test_dispute_invalid_transaction_id() -> Result<()> {
        let reader =
            open_file_read_csv("test-files/dispute_invalid_transaction_id.csv".to_string())?;
        let mut db = Database::default();
        run_engine(reader, &mut db)?;
        assert_eq!(db.transactions.len(), 2);
        assert_eq!(db.clients.len(), 1);
        assert_eq!(db.clients[&1].available, 3.0);
        assert_eq!(db.clients[&1].held, 0.0);
        Ok(())
    }
    #[test]
    fn test_dispute_withdrawal() -> Result<()> {
        let reader = open_file_read_csv("test-files/dispute_withdrawal.csv".to_string())?;
        let mut db = Database::default();
        run_engine(reader, &mut db)?;
        assert_eq!(db.transactions.len(), 3);
        assert_eq!(db.clients.len(), 1);
        assert_eq!(db.clients[&1].available, 1.0);
        assert_eq!(db.clients[&1].held, 1.5);
        Ok(())
    }

    #[test]
    fn test_dispute_client_mismatch() -> Result<()> {
        let reader = open_file_read_csv("test-files/dispute_client_mismatch.csv".to_string())?;
        let mut db = Database::default();
        run_engine(reader, &mut db)?;
        assert_eq!(db.transactions.len(), 2);
        assert_eq!(db.clients.len(), 2);
        assert_eq!(db.clients[&1].available, 1.0);
        assert_eq!(db.clients[&1].held, 0.0);
        assert_eq!(db.clients[&2].available, 2.0);
        assert_eq!(db.clients[&2].held, 0.0);
        Ok(())
    }

    #[test]
    fn test_resolve_disputed_deposit() -> Result<()> {
        let reader = open_file_read_csv("test-files/resolved_dispute.csv".to_string())?;
        let mut db = Database::default();
        run_engine(reader, &mut db)?;
        assert_eq!(db.transactions.len(), 2);
        assert_eq!(db.clients.len(), 1);
        assert_eq!(db.clients[&1].available, 3.0);
        assert_eq!(db.clients[&1].held, 0.0);
        Ok(())
    }
    #[test]
    fn test_resolved_non_disputed() -> Result<()> {
        let reader = open_file_read_csv("test-files/resolved_non_disputed.csv".to_string())?;
        let mut db = Database::default();
        run_engine(reader, &mut db)?;
        assert_eq!(db.transactions.len(), 2);
        assert_eq!(db.clients.len(), 1);
        assert_eq!(db.clients[&1].available, 3.0);
        assert_eq!(db.clients[&1].held, 0.0);
        Ok(())
    }

    #[test]
    fn test_chargeback_dispute() -> Result<()> {
        let reader = open_file_read_csv("test-files/chargeback_dispute.csv".to_string())?;
        let mut db = Database::default();
        run_engine(reader, &mut db)?;
        assert_eq!(db.transactions.len(), 2);
        assert_eq!(db.clients.len(), 1);
        assert_eq!(db.clients[&1].available, 2.0);
        assert_eq!(db.clients[&1].held, 0.0);
        assert_eq!(db.clients[&1].locked, true);
        Ok(())
    }
}
