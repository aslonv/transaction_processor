use anyhow::Result;
use rust_decimal::Decimal;
use serde::{
    de::{self, Deserializer},
    Deserialize,
};

#[derive(Debug, Clone, PartialEq)]
pub enum OperationType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

impl<'de> Deserialize<'de> for OperationType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "deposit" => Ok(OperationType::Deposit),
            "withdrawal" => Ok(OperationType::Withdrawal),
            "dispute" => Ok(OperationType::Dispute),
            "resolve" => Ok(OperationType::Resolve),
            "chargeback" => Ok(OperationType::Chargeback),
            _ => Err(de::Error::unknown_variant(
                &s,
                &["deposit", "withdrawal", "dispute", "resolve", "chargeback"],
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct OperationRecord {
    pub r#type: OperationType,
    pub client: u16,
    pub tx: u32,
    pub amount: Option<Decimal>,
}

#[derive(Debug, Clone)]
pub struct TransactionState {
    pub client: u16,
    pub amount: Decimal,
    pub is_deposit: bool,
}

#[derive(Debug, Clone)]
pub struct ClientBalance {
    pub available: Decimal,
    pub held: Decimal,
    pub locked: bool,
}

impl ClientBalance {
    pub fn new() -> Self {
        Self {
            available: Decimal::ZERO,
            held: Decimal::ZERO,
            locked: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use csv::ReaderBuilder;
    use rust_decimal_macros::dec;
    use std::io::Cursor;

    #[test]
    fn test_deserialization() {
        let data = "type,client,tx,amount\nDeposit,1,1,1.2345";
        let mut rdr = ReaderBuilder::new().from_reader(Cursor::new(data));
        let rec: OperationRecord = rdr.deserialize().next().unwrap().unwrap();
        assert_eq!(rec.r#type, OperationType::Deposit);
        assert_eq!(rec.client, 1);
        assert_eq!(rec.tx, 1);
        assert_eq!(rec.amount, Some(dec!(1.2345)));
    }

    #[test]
    fn test_case_insensitive_type() {
        let data = "type,client,tx,amount\ndeposit,1,1,1.0\nWITHDRAWAL,2,2,2.0";
        let mut rdr = ReaderBuilder::new().from_reader(Cursor::new(data));
        let rec1: OperationRecord = rdr.deserialize().next().unwrap().unwrap();
        let rec2: OperationRecord = rdr.deserialize().next().unwrap().unwrap();
        assert_eq!(rec1.r#type, OperationType::Deposit);
        assert_eq!(rec2.r#type, OperationType::Withdrawal);
    }

    #[test]
    fn test_missing_amount() {
        let data = "type,client,tx\ndispute,1,1";
        let mut rdr = ReaderBuilder::new().from_reader(Cursor::new(data));
        let rec: OperationRecord = rdr.deserialize().next().unwrap().unwrap();
        assert_eq!(rec.amount, None);
    }
}
