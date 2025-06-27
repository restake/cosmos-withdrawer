# Cosmos Withdrawer

A Rust-based CLI tool for automated withdrawal of staking rewards and validator commissions on Cosmos SDK-based blockchain networks. This tool uses the authz module to enable secure delegation of withdrawal permissions between accounts.

## Features

- **Automated Reward Withdrawal**: Withdraw staking rewards and validator commissions automatically
- **Multi-Chain Support**: Works with any Cosmos SDK-based network
- **Flexible Authentication**: Supports both standard secp256k1 and Ethereum-style eth\_secp256k1 keys
- **Threshold-Based Withdrawals**: Only withdraw when rewards exceed specified thresholds
- **Authz Integration**: Uses Cosmos authz module for secure permission delegation
- **Gas Optimization**: Automatic gas estimation with customizable adjustment factors
- **Transaction Generation**: Can generate unsigned transactions for external signing

## Installation

### From Source

Ensure you have Rust 1.85.0 or later installed:

```bash
git clone https://github.com/restake/cosmos-withdrawer
cd cosmos-withdrawer
cargo build --release
```

The binary will be available at `target/release/cosmos-withdrawer`.

## Quick Start

### 1. Setup Validator Operator

First, set up the authorization grants between your delegator (validator) and controller accounts:

```bash
cosmos-withdrawer setup-valoper \
  --rpc-url "https://rpc.osmosis.zone" \
  --delegator-address "osmo1validator..." \
  --delegator-mnemonic "your validator mnemonic phrase" \
  --controller-address "osmo1controller..." \
  --controller-mnemonic "your controller mnemonic phrase"
```

### 2. Withdraw Rewards

Once set up, withdraw rewards when they exceed specified thresholds:

```bash
cosmos-withdrawer withdraw \
  --rpc-url "https://rpc.osmosis.zone" \
  --delegator-address "osmo1validator..." \
  --controller-address "osmo1controller..." \
  --controller-mnemonic "your controller mnemonic phrase" \
  --threshold "1000000uosmo"
```

## Configuration

### Environment Variables

You can configure the tool using environment variables:

```bash
export COSMOS_WITHDRAWER_RPC_URL="https://rpc.osmosis.zone"
export COSMOS_WITHDRAWER_DELEGATOR_ADDRESS="osmo1validator..."
export COSMOS_WITHDRAWER_CONTROLLER_ADDRESS="osmo1controller..."
export COSMOS_WITHDRAWER_DELEGATOR_MNEMONIC="your validator mnemonic"
export COSMOS_WITHDRAWER_CONTROLLER_MNEMONIC="your controller mnemonic"
export COSMOS_WITHDRAWER_WITHDRAW_THRESHOLDS="1000000uosmo,500000uion"
```

### Configuration File

Create a `.envrc.local` file (use [direnv](https://direnv.net)) in the project root for local overrides:

```bash
# .envrc.local
export COSMOS_WITHDRAWER_RPC_URL="http://localhost:26657"
export COSMOS_WITHDRAWER_DELEGATOR_MNEMONIC="your local test mnemonic"
```

## Usage

### Commands

#### `setup-valoper`

Set up authorization grants between delegator and controller accounts.

```bash
cosmos-withdrawer setup-valoper [OPTIONS] <METHOD>
```

**Methods:**
- `auto`: Automatically determine the best setup method
- `authz-withdraw`: Use authz with withdraw address setting (recommended)
- `authz-send`: Use authz with token sending (fallback for older chains)

**Key Options:**
- `--delegator-address`: The validator operator address
- `--controller-address`: The account that will execute withdrawals
- `--reward-address`: Optional separate address to receive rewards
- `--expiration`: Set expiration for authz grants (if required by chain)

#### `withdraw`

Withdraw validator rewards and commissions.

```bash
cosmos-withdrawer withdraw [OPTIONS]
```

**Key Options:**
- `--threshold`: Token thresholds for withdrawal (format: `1000000uosmo`)
- `--gas`: Gas limit (`auto` or specific amount)
- `--gas-prices`: Gas prices (format: `0.025uosmo`)
- `--dry-run`: Simulate without broadcasting
- `--generate-only`: Generate unsigned transaction JSON

#### `debug`

Debug utilities for address derivation and testing.

```bash
cosmos-withdrawer debug derive-address \
  --mnemonic "your mnemonic phrase" \
  --key-type secp256k1 \
  --coin-type 118
```

### Account Types

The tool supports different key types:

- `secp256k1`: Standard Cosmos SDK key type (default)
- `eth_secp256k1`: Ethereum-style keys (for Evmos, Injective, etc.)

### Gas Configuration

#### Automatic Gas Estimation

```bash
--gas auto --gas-adjustment 1.25
```

#### Manual Gas Setting

```bash
--gas 200000 --gas-prices 0.025uosmo
```

### Transaction Modes

#### Normal Execution
Transactions are signed and broadcasted automatically.

#### Generate Only
Generate unsigned transaction JSON for external signing:

```bash
cosmos-withdrawer withdraw --generate-only > unsigned_tx.json
```

#### Dry Run
Simulate transactions without broadcasting:

```bash
cosmos-withdrawer withdraw --dry-run
```

## Architecture

### Account Roles

1. **Delegator Account**: The validator operator account that receives rewards
2. **Controller Account**: The account authorized to execute withdrawals
3. **Reward Account**: Optional separate account to receive withdrawn funds

### Authorization Flow

1. **Setup Phase**: Delegator grants authz permissions to controller
2. **Withdrawal Phase**: Controller executes withdrawals on behalf of delegator
3. **Distribution**: Rewards are sent to the configured reward address

### Supported Networks

The tool includes gas price data for 200+ Cosmos networks. For networks not in the built-in registry, specify gas prices manually:

```bash
--gas-prices 0.025uatom
```

## Examples

### Osmosis Validator Setup

```bash
# Setup authz grants
cosmos-withdrawer setup-valoper \
  --rpc-url "https://rpc.osmosis.zone" \
  --delegator-address "osmo1validator..." \
  --controller-address "osmo1controller..." \
  --delegator-mnemonic "$VALIDATOR_MNEMONIC" \
  --controller-mnemonic "$CONTROLLER_MNEMONIC"

# Withdraw when rewards exceed 1 OSMO
cosmos-withdrawer withdraw \
  --rpc-url "https://rpc.osmosis.zone" \
  --delegator-address "osmo1validator..." \
  --controller-address "osmo1controller..." \
  --controller-mnemonic "$CONTROLLER_MNEMONIC" \
  --threshold "1000000uosmo"
```

### Multi-Token Thresholds

```bash
cosmos-withdrawer withdraw \
  --threshold "1000000uosmo" \
  --threshold "500000uion" \
  --threshold "2000000uakt"
```

### Evmos/Ethereum-style Keys

```bash
cosmos-withdrawer setup-valoper \
  --delegator-address-type eth_secp256k1 \
  --controller-address-type eth_secp256k1 \
  --delegator-mnemonic-coin-type 60 \
  # ... other options
```

## Security Considerations

### Key Management

- **Never share mnemonics**: Store them securely and never commit to version control
- **Use environment variables**: Avoid passing mnemonics as command-line arguments
- **Separate accounts**: Use different accounts for validator operations and withdrawals

### Authz Permissions

The tool grants minimal required permissions:

- `MsgWithdrawDelegatorReward`: For reward withdrawals
- `MsgWithdrawValidatorCommission`: For commission withdrawals
- `MsgSetWithdrawAddress`: For setting reward destination (preferred)
- `MsgSend`: Only when withdraw address setting is not supported (fallback)

### Network Security

- **Use HTTPS RPCs**: Always use secure RPC endpoints
- **Verify chain IDs**: Ensure you're connected to the correct network
- **Monitor transactions**: Review all transactions before and after execution

## Troubleshooting

### Common Issues

#### "Account not found"
Ensure all accounts are initialized with some balance before setup.

#### "Insufficient gas"
Increase gas limit or adjustment factor:
```bash
--gas-adjustment 1.5
```

#### "Unknown chain ID"
Specify gas prices manually for unsupported chains:
```bash
--gas-prices 0.025unative
```

#### "Invalid mnemonic"
Verify mnemonic phrase and coin type match your wallet configuration.

### Debug Commands

Check derived addresses:
```bash
cosmos-withdrawer debug derive-address \
  --mnemonic "your mnemonic" \
  --key-type secp256k1 \
  --coin-type 118
```

## Development

### Building

```bash
cargo build
```

### Testing

```bash
cargo test
```

### Chain Registry Data

Update gas price data:
```bash
./hack/chain-registry-data.sh > src/chain_registry/gas_data.json
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

This project is licensed under the terms specified in the repository.

## Support

For support and questions:
- Create an issue on GitHub
- Check existing issues for solutions
- Review the troubleshooting section

---

**⚠️ Disclaimer**: This tool handles private keys and executes blockchain transactions. Always test on testnets first and never use with funds you cannot afford to lose. Review all transactions before execution.
