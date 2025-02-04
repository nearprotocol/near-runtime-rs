# Factory Contract

Example implementation of a factory contract using [near-sdk-rs].

[near-sdk-rs]: https://github.com/near/near-sdk-rs

NOTES:

- This example demonstrates how to create and manage multiple instances of a contract from a factory contract.
- Ensure that the factory contract is deployed and accessible.

## Project Structure

- `.cargo`: Configuration files for Cargo, the Rust package manager.
- `high-level/src`: Source code for the high-level factory contract example.
- `low-level/src`: Source code for the low-level factory contract example.
- `res`: Compiled contract binaries.
- `tests`: Integration tests for the factory contract examples.

## Building

To build run:

```bash
./build.sh
```

## Testing

To test run:

```bash
cargo test --package factory-contract -- --nocapture
```

## Usage

## Usage

1. Prerequisite: Install cargo-near:

```bash
cargo install cargo-near
```

2. Create a new testnet account:

```bash
cargo near create-dev-account
```

3. Deploy the factory contract:

```bash
cargo cargo build --target wasm32-unknown-unknown --release
near dev-deploy --wasmFile target/wasm32-unknown-unknown/release/factory_contract.wasm
```

4. Use the factory contract to create instances of the target contract:

```bash
near contract call-function as-transaction your-factory-contract-account.testnet deploy_status_message json-args '{"account_id": "sub.your-factory-contract-account.testnet"}' prepaid-gas '100 Tgas' attached-deposit '0 NEAR'
```

5. Demonstrate calls to simple_call and complex_call functions:

- Call simple_call function:

```bash
near call your-factory-contract-account.testnet simple_call '{"arg1": "value1"}' --accountId your-account.testnet
```

- Call complex_call function:

```bash
near call your-factory-contract-account.testnet complex_call '{"arg1": "value1", "arg2": "value2"}' --accountId your-account.testnet
```

NOTE FOR 3, 4 AND 5: Replace target/wasm32-unknown-unknown/release/factory_contract.wasm with the actual path to your compiled WebAssembly file, and your-factory-contract-account.testnet with your actual NEAR testnet account ID.

## Dependencies

- [near-sdk-rs](https://github.com/near/near-sdk-rs): NEAR Protocol's Rust SDK.
- [Cargo](https://doc.rust-lang.org/cargo/): Rust's package manager.

## Changelog

### `1.0.0`

- Initial implementation of factory contract functionality.
