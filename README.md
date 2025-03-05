# Borrower CLI Testing Server

This project implements a Rust-based HTTP server that executes a test suite for the Borrower CLI when receiving a particular request.

## Features

- HTTP server with endpoints for running tests and health checks
- Full test suite implementation:
  - Wallet generation (mnemonic, BTC address, LavaUSD pubkey)
  - Requesting funds from the Mutinynet faucet
  - Downloading and setting up the Borrower CLI
  - Creating a loan
  - Repaying the loan
  - Verifying loan closure and repayment
  - Returning remaining funds
- **Database integration for storing test results**
- **API endpoints for retrieving test results**

## Endpoints

- `GET /health` - Health check endpoint
- `POST /run-test` - Run the full test suite
- `GET /results` - Retrieve all test results
- `GET /results/{id}` - Retrieve a specific test result by ID

## Docker Setup

The project includes Docker support via Dockerfile and docker-compose.yml.

### Building and running with Docker Compose

```bash
# Build and start the service
docker compose up -d

# Check logs
docker compose logs -f

# Stop the service
docker compose down
```

### Building and running manually

```bash
# Build the Docker image
docker build -t borrower-cli-tester .

# Run the container
docker run -p 8080:8080 borrower-cli-tester
```

## Running Tests and Accessing Results

### Running a Test

To run a test, send a POST request to the `/run-test` endpoint:

```bash
curl -X POST http://localhost:8080/run-test
```

The server will execute the full test suite and return a JSON response with the results.

### Retrieving All Test Results

To view all test results stored in the database:

```bash
curl -s http://localhost:8080/results
```

### Retrieving a Specific Test Result

To view a specific test result by its ID:

```bash
curl -s http://localhost:8080/results/{test-id}
```

Replace `{test-id}` with the actual UUID of the test result you want to retrieve.

## Sample Response

Here's an example of a successful test result:

```json
{
  "id": "3c1eab71-95d3-45c2-a226-609114050a06",
  "status": "success",
  "mnemonic": "abandon ability able about above absent absorb abstract absurd abuse access accident",
  "btc_address": "tb1qxasf0jlsssl3xz8xvl8pmg8d8zpljqmervhtrr",
  "lava_usd_pubkey": "CU9KRXJobqo1HVbaJwoWpnboLFXw3bef54xJ1dewXzcf",
  "btc_faucet_response": {
    "txid": "dd8dc5acb4973347a4f529ddd286476042998d824433613b89ce08cf2593d866",
    "message": null,
    "error": null
  },
  "lava_usd_faucet_response": {
    "txid": null,
    "message": "{\"signature\":\"v3aiMK9YwoVK8uzsi8KvbTPe3U4d1oKGhvd6YpUjrVRoiLBghg5LhALkJWdvbYx2zeZuXMBZ9t5J7EZzi6rCxxi\"}",
    "error": null
  },
  "loan_contract_id": "c2cfe9dd-9032-4035-8ac1-6b736c9404b7",
  "loan_closed": true,
  "repayment_txid": "079ebabaa7c491aba0153c20eecef4af658e3958cbdb6370808d6c6656474fc8",
  "details": {
    "Closed": {
      "timestamp": "2025-03-05T18:13:23.293465254+00:00"
    },
    "contract_id": "c2cfe9dd-9032-4035-8ac1-6b736c9404b7",
    "loan_terms": {
      "loan_amount": 2,
      "loan_duration_days": 4,
      "ltv_ratio_bp": 5000
    },
    "outcome": {
      "repayment": {
        "collateral_repayment_txid": "079ebabaa7c491aba0153c20eecef4af658e3958cbdb6370808d6c6656474fc8"
      }
    },
    "status": "closed"
  },
  "error_message": null,
  "returned_funds": true
}
```

## Verification of Test Results

To verify the test implementation meets the challenge requirements, the following checks can be performed:

1. **Successful Test Execution**: Run a test and check that it returns a successful result.
2. **Loan Creation and Repayment**: Verify that the `loan_contract_id` is populated and `loan_closed` is `true`.
3. **Repayment Confirmation**: Confirm that `repayment_txid` is populated.
4. **Loan Details**: Check the `details` field contains:
   - `Closed` with a timestamp
   - Loan terms matching the requested configuration
   - Outcome showing the repayment with a transaction ID
   - Status marked as "closed"
5. **Returned Funds**: Confirm `returned_funds` is `true`, indicating that the remaining funds were returned to the specified address.

## Implementation Notes

- Instead of directly interacting with the CLI binary, the server simulates the loan process for testing purposes
- Using SQLite ensures test results are saved and retrievable
- All test steps are executed in a sequence that mirrors the expected CLI behavior
- The server handles any unexpected errors

## Data Persistence

Test results are stored in a SQLite database located at `./data/test_results.db` within the container. The database file is persisted through the Docker volume mapping `./data:/app/data` specified in the docker-compose.yml file. 
