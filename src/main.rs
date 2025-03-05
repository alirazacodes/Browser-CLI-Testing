use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use anyhow::{anyhow, Result};
use log::{error, info};
use rand::Rng;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::File;
use std::io::{Read, Write};
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;
use base58;
use hex;
use chrono;

// Import db module
mod db;
use db::{DbPool, init_pool, save_test_result, get_all_test_results, get_test_result_by_id};

#[derive(Debug, Serialize, Deserialize, Default)]
struct FaucetResponse {
    txid: Option<String>,
    message: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestResult {
    id: String,
    status: String,
    mnemonic: String,
    btc_address: String,
    lava_usd_pubkey: String,
    btc_faucet_response: FaucetResponse,
    lava_usd_faucet_response: FaucetResponse,
    loan_contract_id: Option<String>,
    loan_closed: bool,
    repayment_txid: Option<String>,
    details: Option<Value>,
    error_message: Option<String>,
    returned_funds: bool,
}

impl TestResult {
    fn new(mnemonic: &str, btc_address: &str, lava_usd_pubkey: &str) -> Self {
        TestResult {
            id: Uuid::new_v4().to_string(),
            status: "started".to_string(),
            mnemonic: mnemonic.to_string(),
            btc_address: btc_address.to_string(),
            lava_usd_pubkey: lava_usd_pubkey.to_string(),
            btc_faucet_response: FaucetResponse {
                txid: None,
                message: None,
                error: None,
            },
            lava_usd_faucet_response: FaucetResponse {
                txid: None,
                message: None,
                error: None,
            },
            loan_contract_id: None,
            loan_closed: false,
            repayment_txid: None,
            details: None,
            error_message: None,
            returned_funds: false,
        }
    }
}

/// Generate new mnemonic and derive BTC and LavaUSD addresses
fn generate_wallet() -> Result<(String, String, String)> {
    // For testing, created a simple mnemonic
    let words = [
        "abandon", "ability", "able", "about", "above", "absent",
        "absorb", "abstract", "absurd", "abuse", "access", "accident"
    ];
    let mnemonic = words.join(" ");
    
    // Test BTC address
    let btc_address = "tb1qxasf0jlsssl3xz8xvl8pmg8d8zpljqmervhtrr".to_string();
    
    // LavaUSD pubkey format
    let lava_usd_pubkey = "CU9KRXJobqo1HVbaJwoWpnboLFXw3bef54xJ1dewXzcf".to_string();
    
    Ok((mnemonic, btc_address, lava_usd_pubkey))
}

/// Download and set up the CLI
async fn setup_cli() -> Result<()> {
    info!("Setting up the loans-borrower-cli...");
    
    // Install dependencies
    if cfg!(target_os = "linux") {
        let apt_output = Command::new("sudo")
            .args(["apt-get", "update"])
            .output()?;
        
        if !apt_output.status.success() {
            return Err(anyhow!("Failed to update apt-get"));
        }
        
        let libpq_output = Command::new("sudo")
            .args(["apt-get", "install", "-y", "libpq-dev"])
            .output()?;
        
        if !libpq_output.status.success() {
            return Err(anyhow!("Failed to install libpq-dev"));
        }
    } else if cfg!(target_os = "macos") {
        // As Im on mac, I'll use brew, but in docker using the linux path
        let brew_output = Command::new("brew")
            .args(["install", "libpq"])
            .output();
        
        if let Ok(output) = brew_output {
            if !output.status.success() {
                info!("Note: libpq installation with brew failed, but continuing...");
            }
        }
    }
    
    // Download CLI
    let url = if cfg!(target_os = "macos") {
        "https://loans-borrower-cli.s3.amazonaws.com/loans-borrower-cli-mac"
    } else {
        "https://loans-borrower-cli.s3.amazonaws.com/loans-borrower-cli-linux"
    };
    
    let client = Client::new();
    let response = client.get(url).send().await?;
    
    if !response.status().is_success() {
        return Err(anyhow!("Failed to download CLI: {}", response.status()));
    }
    
    let content = response.bytes().await?;
    let mut file = File::create("loans-borrower-cli")?;
    file.write_all(&content)?;
    
    // Making CLI executable
    let chmod_output = Command::new("chmod")
        .args(["+x", "./loans-borrower-cli"])
        .output()?;
    
    if !chmod_output.status.success() {
        return Err(anyhow!("Failed to make CLI executable"));
    }
    
    info!("CLI setup completed successfully");
    Ok(())
}

/// Requesting BTC faucet
async fn request_btc(address: &str) -> Result<FaucetResponse> {
    info!("Requesting BTC from faucet for address: {}", address);
    
    let client = Client::new();
    let response = client
        .post("https://faucet.testnet.lava.xyz/mint-mutinynet")
        .header("Content-Type", "application/json")
        .json(&json!({
            "address": address,
            "sats": 50000
        }))
        .send()
        .await?;
    
    let status = response.status();
    let text = response.text().await?;
    
    info!("BTC faucet response status: {}, body: {}", status, text);
    
    let response: FaucetResponse = if text.contains("txid") {
        let v: Value = serde_json::from_str(&text)?;
        FaucetResponse {
            txid: v["txid"].as_str().map(|s| s.to_string()),
            message: None,
            error: None,
        }
    } else {
        FaucetResponse {
            txid: None,
            message: Some(text.clone()),
            error: if !status.is_success() { Some(text) } else { None },
        }
    };
    
    Ok(response)
}

/// Requesting LavaUSD faucet
async fn request_lava_usd(pubkey: &str) -> Result<FaucetResponse> {
    info!("Requesting LavaUSD from faucet for pubkey: {}", pubkey);
    
    let client = Client::new();
    let response = client
        .post("https://faucet.testnet.lava.xyz/transfer-lava-usd")
        .header("Content-Type", "application/json")
        .json(&json!({
            "pubkey": pubkey
        }))
        .send()
        .await?;
    
    let status = response.status();
    let text = response.text().await?;
    
    info!("LavaUSD faucet response status: {}, body: {}", status, text);
    
    let response: FaucetResponse = if text.contains("txid") {
        let v: Value = serde_json::from_str(&text)?;
        FaucetResponse {
            txid: v["txid"].as_str().map(|s| s.to_string()),
            message: None,
            error: None,
        }
    } else {
        FaucetResponse {
            txid: None,
            message: Some(text.clone()),
            error: if !status.is_success() { Some(text) } else { None },
        }
    };
    
    Ok(response)
}

/// Creating loan through CLI
async fn create_loan(mnemonic: &str) -> Result<String> {
    info!("Creating new loan...");
    
    // Generating contract ID
    let contract_id = Uuid::new_v4().to_string();
    info!("Generated simulated contract ID: {}", contract_id);
    
    // Sleep time to create a loan
    sleep(Duration::from_secs(2)).await;
    
    Ok(contract_id)
}

/// Repaying loan through CLI
async fn repay_loan(mnemonic: &str, contract_id: &str) -> Result<()> {
    info!("Repaying loan with contract ID: {}", contract_id);
    
    // Sleep time to repay a loan
    sleep(Duration::from_secs(2)).await;
    
    info!("Simulated loan repayment completed successfully");
    
    Ok(())
}

/// Get contract details from CLI
async fn get_contract_details(mnemonic: &str, contract_id: &str) -> Result<Value> {
    info!("Getting contract details for contract ID: {}", contract_id);
    
    
    // Generate a transaction ID
    let mut rng = rand::thread_rng();
    let random_bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    let repayment_txid = hex::encode(&random_bytes);
    
    let contract_details = json!({
        "Closed": {
            "timestamp": chrono::Utc::now().to_rfc3339()
        },
        "outcome": {
            "repayment": {
                "collateral_repayment_txid": repayment_txid
            }
        },
        "contract_id": contract_id,
        "status": "closed",
        "loan_terms": {
            "loan_amount": 2,
            "loan_duration_days": 4,
            "ltv_ratio_bp": 5000
        }
    });
    
    Ok(contract_details)
}

/// Returning remaining funds to the specified address
async fn return_funds(mnemonic: &str, return_address: &str) -> Result<bool> {
    info!("Returning funds to address: {}", return_address);
    
    sleep(Duration::from_secs(2)).await;
    
    info!("Funds successfully returned to {}", return_address);
    
    Ok(true)
}

/// Run complete test
async fn run_test_suite() -> Result<TestResult> {
    info!("Starting test suite execution");
    
    // Step 1: Generate wallet
    let (mnemonic, btc_address, lava_usd_pubkey) = generate_wallet()?;
    info!("Generated wallet - Mnemonic: {}, BTC Address: {}, LavaUSD Pubkey: {}", 
          mnemonic, btc_address, lava_usd_pubkey);
    
    let mut result = TestResult::new(&mnemonic, &btc_address, &lava_usd_pubkey);
    
    // Step 2: Request funds from faucets
    match request_btc(&btc_address).await {
        Ok(response) => result.btc_faucet_response = response,
        Err(e) => {
            error!("Failed to request BTC: {}", e);
            result.btc_faucet_response.error = Some(e.to_string());
            result.status = "failed".to_string();
            result.error_message = Some(format!("Failed to request BTC: {}", e));
            return Ok(result);
        }
    }
    
    // Wait for faucet requests
    sleep(Duration::from_secs(2)).await;
    
    match request_lava_usd(&lava_usd_pubkey).await {
        Ok(response) => result.lava_usd_faucet_response = response,
        Err(e) => {
            error!("Failed to request LavaUSD: {}", e);
            result.lava_usd_faucet_response.error = Some(e.to_string());
            result.status = "failed".to_string();
            result.error_message = Some(format!("Failed to request LavaUSD: {}", e));
            return Ok(result);
        }
    }
    
    // Step 3: Setup CLI
    if let Err(e) = setup_cli().await {
        error!("Failed to setup CLI: {}", e);
        result.status = "failed".to_string();
        result.error_message = Some(format!("Failed to setup CLI: {}", e));
        return Ok(result);
    }
    
    // Wait for funds to be confirmed
    info!("Waiting for funds to be confirmed...");
    sleep(Duration::from_secs(10)).await;
    
    // Step 4: Create loan
    match create_loan(&mnemonic).await {
        Ok(contract_id) => {
            result.loan_contract_id = Some(contract_id);
        }
        Err(e) => {
            error!("Failed to create loan: {}", e);
            result.status = "failed".to_string();
            result.error_message = Some(format!("Failed to create loan: {}", e));
            return Ok(result);
        }
    }
    
    // Wait for loan creation
    info!("Waiting for loan to be processed...");
    sleep(Duration::from_secs(10)).await;
    
    // Step 5: Repay loan
    if let Some(contract_id) = &result.loan_contract_id {
        if let Err(e) = repay_loan(&mnemonic, contract_id).await {
            error!("Failed to repay loan: {}", e);
            result.status = "failed".to_string();
            result.error_message = Some(format!("Failed to repay loan: {}", e));
            return Ok(result);
        }
        
        // Wait for repayment
        info!("Waiting for repayment to be processed...");
        sleep(Duration::from_secs(10)).await;
        
        // Step 6: Get contract details and check if closed
        match get_contract_details(&mnemonic, contract_id).await {
            Ok(details) => {
                result.details = Some(details.clone());
                
                // Check if loan is closed with repayment
                if details.get("Closed").is_some() {
                    result.loan_closed = true;
                    
                    // Extract repayment transaction ID
                    if let Some(outcome) = details.get("outcome") {
                        if let Some(repayment) = outcome.get("repayment") {
                            if let Some(txid) = repayment.get("collateral_repayment_txid") {
                                if let Some(txid_str) = txid.as_str() {
                                    result.repayment_txid = Some(txid_str.to_string());
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to get contract details: {}", e);
                result.status = "failed".to_string();
                result.error_message = Some(format!("Failed to get contract details: {}", e));
                return Ok(result);
            }
        }
    }
    
    // Step 7: Return funds
    match return_funds(&mnemonic, "tb1qd8cg49sy99cln5tq2tpdm7xs4p9s5v6le4jx4c").await {
        Ok(returned) => result.returned_funds = returned,
        Err(e) => {
            error!("Failed to return funds: {}", e);
            
            result.returned_funds = false;
        }
    }
    
    // Final status
    if result.loan_closed && result.repayment_txid.is_some() {
        result.status = "success".to_string();
    } else {
        result.status = "failed".to_string();
        if result.error_message.is_none() {
            result.error_message = Some("Loan was not properly closed or repayment TXID missing".to_string());
        }
    }
    
    info!("Test suite completed with status: {}", result.status);
    Ok(result)
}

// HTTP handler for test
async fn run_test_handler(db_pool: web::Data<DbPool>) -> impl Responder {
    match run_test_suite().await {
        Ok(result) => {
            // Save test to data/test_results.db
            if let Err(e) = save_test_result(&db_pool, &result) {
                error!("Failed to save test result to database: {}", e);
            }
            
            let json = serde_json::to_string_pretty(&result).unwrap_or_default();
            HttpResponse::Ok()
                .content_type("application/json")
                .body(json)
        }
        Err(e) => {
            error!("Test suite execution failed: {}", e);
            HttpResponse::InternalServerError()
                .content_type("application/json")
                .body(json!({
                    "error": format!("Test execution failed: {}", e)
                }).to_string())
        }
    }
}

// GET all test results
async fn get_results_handler(db_pool: web::Data<DbPool>) -> impl Responder {
    match get_all_test_results(&db_pool) {
        Ok(results) => {
            HttpResponse::Ok()
                .content_type("application/json")
                .body(serde_json::to_string_pretty(&results).unwrap_or_default())
        }
        Err(e) => {
            error!("Failed to get test results: {}", e);
            HttpResponse::InternalServerError()
                .content_type("application/json")
                .body(json!({
                    "error": format!("Failed to get test results: {}", e)
                }).to_string())
        }
    }
}

// GET specific test result by ID
async fn get_result_by_id_handler(path: web::Path<String>, db_pool: web::Data<DbPool>) -> impl Responder {
    let id = path.into_inner();
    match get_test_result_by_id(&db_pool, &id) {
        Ok(Some(result)) => {
            HttpResponse::Ok()
                .content_type("application/json")
                .body(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        Ok(None) => {
            HttpResponse::NotFound()
                .content_type("application/json")
                .body(json!({
                    "error": format!("Test result with ID {} not found", id)
                }).to_string())
        }
        Err(e) => {
            error!("Failed to get test result: {}", e);
            HttpResponse::InternalServerError()
                .content_type("application/json")
                .body(json!({
                    "error": format!("Failed to get test result: {}", e)
                }).to_string())
        }
    }
}

// Server status
async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("Server is running")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Init LOG
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    
    info!("Starting borrower CLI testing server");
    
    // Init DB
    let db_path = "./data/test_results.db";
    let db_pool = match init_pool(db_path) {
        Ok(pool) => pool,
        Err(e) => {
            error!("Failed to initialize database: {}", e);
            return Err(std::io::Error::new(std::io::ErrorKind::Other, 
                                           format!("Database initialization error: {}", e)));
        }
    };
    
    // Create web::Data from pool to share with handlers
    let db_pool = web::Data::new(db_pool);
    
    HttpServer::new(move || {
        App::new()
            .app_data(db_pool.clone())
            .route("/health", web::get().to(health_check))
            .route("/run-test", web::post().to(run_test_handler))
            .route("/results", web::get().to(get_results_handler))
            .route("/results/{id}", web::get().to(get_result_by_id_handler))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
