use anyhow::Result;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::path::Path;
use log::info;
use serde_json::Value;

use crate::TestResult;

pub type DbPool = Pool<SqliteConnectionManager>;

/// Init DB pool
pub fn init_pool(db_path: &str) -> Result<DbPool> {
    if let Some(parent) = Path::new(db_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let manager = SqliteConnectionManager::file(db_path);
    let pool = Pool::new(manager)?;
    
    // DB schema
    let conn = pool.get()?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS test_results (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            status TEXT NOT NULL,
            mnemonic TEXT NOT NULL,
            btc_address TEXT NOT NULL,
            lava_usd_pubkey TEXT NOT NULL,
            btc_faucet_response TEXT NOT NULL,
            lava_usd_faucet_response TEXT NOT NULL,
            loan_contract_id TEXT,
            loan_closed INTEGER NOT NULL,
            repayment_txid TEXT,
            details TEXT,
            error_message TEXT,
            returned_funds INTEGER NOT NULL
        )",
        [],
    )?;
    
    info!("Database initialized at {}", db_path);
    Ok(pool)
}

/// Save tests to data/test_results.db
pub fn save_test_result(pool: &DbPool, result: &TestResult) -> Result<()> {
    let conn = pool.get()?;
    
    conn.execute(
        "INSERT INTO test_results (
            id, timestamp, status, mnemonic, btc_address, lava_usd_pubkey,
            btc_faucet_response, lava_usd_faucet_response, loan_contract_id,
            loan_closed, repayment_txid, details, error_message, returned_funds
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            result.id,
            chrono::Utc::now().to_rfc3339(),
            result.status,
            result.mnemonic,
            result.btc_address,
            result.lava_usd_pubkey,
            serde_json::to_string(&result.btc_faucet_response)?,
            serde_json::to_string(&result.lava_usd_faucet_response)?,
            result.loan_contract_id,
            result.loan_closed as i64,
            result.repayment_txid,
            result.details.as_ref().map(|d| serde_json::to_string(d).unwrap_or_default()),
            result.error_message,
            result.returned_funds as i64
        ],
    )?;
    
    info!("Test result with ID {} saved to database", result.id);
    Ok(())
}

/// GET tests from data/test_results.db
pub fn get_all_test_results(pool: &DbPool) -> Result<Vec<TestResult>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT * FROM test_results ORDER BY timestamp DESC")?;
    
    let test_result_iter = stmt.query_map([], |row| {
        let btc_faucet_response_str: String = row.get(6)?;
        let lava_usd_faucet_response_str: String = row.get(7)?;
        let details_str: Option<String> = row.get(11)?;
        
        let btc_faucet_response: crate::FaucetResponse = serde_json::from_str(&btc_faucet_response_str)
            .unwrap_or_else(|_| crate::FaucetResponse::default());
        
        let lava_usd_faucet_response: crate::FaucetResponse = serde_json::from_str(&lava_usd_faucet_response_str)
            .unwrap_or_else(|_| crate::FaucetResponse::default());
        
        let details = details_str.and_then(|s| serde_json::from_str(&s).ok());
        
        Ok(TestResult {
            id: row.get(0)?,
            status: row.get(2)?,
            mnemonic: row.get(3)?,
            btc_address: row.get(4)?,
            lava_usd_pubkey: row.get(5)?,
            btc_faucet_response,
            lava_usd_faucet_response,
            loan_contract_id: row.get(8)?,
            loan_closed: row.get::<_, i64>(9)? != 0,
            repayment_txid: row.get(10)?,
            details,
            error_message: row.get(12)?,
            returned_funds: row.get::<_, i64>(13)? != 0,
        })
    })?;
    
    let mut results = Vec::new();
    for result in test_result_iter {
        results.push(result?);
    }
    
    Ok(results)
}

/// GET specific test by ID
pub fn get_test_result_by_id(pool: &DbPool, id: &str) -> Result<Option<TestResult>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT * FROM test_results WHERE id = ?")?;
    
    let mut test_result_iter = stmt.query_map(params![id], |row| {
        let btc_faucet_response_str: String = row.get(6)?;
        let lava_usd_faucet_response_str: String = row.get(7)?;
        let details_str: Option<String> = row.get(11)?;
        
        let btc_faucet_response: crate::FaucetResponse = serde_json::from_str(&btc_faucet_response_str)
            .unwrap_or_else(|_| crate::FaucetResponse::default());
        
        let lava_usd_faucet_response: crate::FaucetResponse = serde_json::from_str(&lava_usd_faucet_response_str)
            .unwrap_or_else(|_| crate::FaucetResponse::default());
        
        let details = details_str.and_then(|s| serde_json::from_str(&s).ok());
        
        Ok(TestResult {
            id: row.get(0)?,
            status: row.get(2)?,
            mnemonic: row.get(3)?,
            btc_address: row.get(4)?,
            lava_usd_pubkey: row.get(5)?,
            btc_faucet_response,
            lava_usd_faucet_response,
            loan_contract_id: row.get(8)?,
            loan_closed: row.get::<_, i64>(9)? != 0,
            repayment_txid: row.get(10)?,
            details,
            error_message: row.get(12)?,
            returned_funds: row.get::<_, i64>(13)? != 0,
        })
    })?;
    
    match test_result_iter.next() {
        Some(result) => Ok(Some(result?)),
        None => Ok(None),
    }
} 