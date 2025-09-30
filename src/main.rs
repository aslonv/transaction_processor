use anyhow::{Context, Result};
use csv::{ReaderBuilder, Writer};
use engine::process_transactions;
use rust_decimal::Decimal;
use std::env;
use std::fs::File;
use std::io::{self};

mod engine;
mod models;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return Err(anyhow::anyhow!("Usage: cargo run -- <input.csv>"));
    }

    let file = File::open(&args[1]).context("Failed to open input file")?;
    let mut rdr = ReaderBuilder::new()
        .trim(csv::Trim::All)
        .flexible(true)
        .from_reader(file);

    let client_balances = process_transactions(&mut rdr)?;

    let mut wtr = Writer::from_writer(io::stdout());
    wtr.write_record(&["client", "available", "held", "total", "locked"])
        .context("Failed to write header")?;

    let mut client_ids: Vec<u16> = client_balances.keys().cloned().collect();
    client_ids.sort();

    for id in client_ids {
        let balance = client_balances.get(&id).unwrap();
        let total = balance.available + balance.held;
        wtr.write_record(&[
            id.to_string(),
            format_decimal(balance.available),
            format_decimal(balance.held),
            format_decimal(total),
            if balance.locked { "true" } else { "false" }.to_string(),
        ])
        .context("Failed to write record")?;
    }

    wtr.flush().context("Failed to flush output")?;
    Ok(())
}

fn format_decimal(value: Decimal) -> String {
    format!("{:.4}", value.round_dp(4))
}
