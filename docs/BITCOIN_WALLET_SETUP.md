# How to Get a Bitcoin Address for Mining

This guide explains how to get a Bitcoin address to receive mining rewards.

## What is a Bitcoin Address?

A Bitcoin address is like a bank account number where you can receive Bitcoin. For mining, you need an address to tell the pool where to send your rewards if you find a block.

Bitcoin addresses look like:
- **Legacy (P2PKH)**: `1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa`
- **SegWit (P2SH)**: `3J98t1WpEZ73CNmYviecrnyiWrnqRhWNLy`
- **Native SegWit (Bech32)**: `bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh` ✅ **Recommended**

## Option 1: Software Wallet (Recommended for Testing)

### Electrum Wallet (Desktop - Most Popular)

**Best for**: Testing, learning, full control

1. **Download Electrum**
   ```bash
   # Ubuntu/Debian
   sudo apt-get install electrum
   
   # Or download from: https://electrum.org/
   ```

2. **Install and Create Wallet**
   - Launch Electrum
   - Choose "Create new wallet"
   - Select "Standard wallet"
   - Choose "Create a new seed"
   - **IMPORTANT**: Write down your 12-word seed phrase on paper!
   - Set a strong password

3. **Get Your Address**
   - Go to "Receive" tab
   - Copy the address (starts with `bc1...`)
   - This is your mining address!

**Pros**: Free, open-source, full control, works offline
**Cons**: You're responsible for security

### BlueWallet (Mobile - Easy)

**Best for**: Beginners, mobile users

1. **Download BlueWallet**
   - iOS: App Store
   - Android: Google Play
   - Website: https://bluewallet.io/

2. **Create Wallet**
   - Open app
   - Tap "Add now"
   - Choose "Bitcoin"
   - **Save your backup phrase!**

3. **Get Address**
   - Tap your wallet
   - Tap "Receive"
   - Copy the address

**Pros**: User-friendly, mobile, free
**Cons**: Mobile security risks

## Option 2: Hardware Wallet (Most Secure)

### Ledger or Trezor

**Best for**: Serious miners, large amounts

Popular hardware wallets:
- **Ledger Nano S/X**: https://www.ledger.com/
- **Trezor One/Model T**: https://trezor.io/

**Cost**: $50-200
**Pros**: Maximum security, offline storage
**Cons**: Costs money, setup complexity

## Option 3: Exchange Wallet (Not Recommended for Mining)

### Coinbase, Binance, Kraken, etc.

**⚠️ WARNING**: Not recommended for mining because:
- You don't control the private keys
- Exchanges may not accept mining deposits
- Risk of account closure
- Higher fees

**Only use if**: You plan to sell Bitcoin immediately

## For Testing BMiner (Quick Start)

### Use a Testnet Address (No Real Bitcoin)

For testing without risk:

1. **Get Testnet Coins**
   - Visit: https://testnet-faucet.mempool.co/
   - Get free testnet Bitcoin

2. **Use Testnet Address**
   - Testnet addresses start with `tb1...` or `m...` or `n...`
   - No real value, perfect for testing

3. **Configure BMiner for Testnet**
   ```toml
   [pool]
   url = "stratum+tcp://testnet.braiins.com:3333"
   username = "tb1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh"
   ```

### Use a Mainnet Address (Real Bitcoin)

For actual mining:

1. **Install Electrum** (see above)
2. **Create wallet and get address**
3. **Use in BMiner config**:
   ```toml
   [pool]
   url = "stratum+tcp://stratum.braiins.com:3333"
   username = "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh"
   ```

## Quick Setup for BMiner Testing

### Method 1: Electrum (5 minutes)

```bash
# Install Electrum
sudo apt-get update
sudo apt-get install electrum

# Launch Electrum
electrum

# Follow wizard:
# 1. Create new wallet
# 2. Standard wallet
# 3. Create new seed
# 4. Write down seed phrase!
# 5. Set password
# 6. Go to Receive tab
# 7. Copy address (bc1...)

# Use address in BMiner config
cp config/braiins-pool-test.toml.example config/braiins-pool-test.toml
nano config/braiins-pool-test.toml
# Replace YOUR_BITCOIN_ADDRESS_HERE with your address
```

### Method 2: Use Demo Address (Testing Only)

For quick testing without setting up a wallet:

```toml
[pool]
url = "stratum+tcp://stratum.braiins.com:3333"
# Demo address (you won't receive any rewards!)
username = "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh"
```

**⚠️ Note**: This is just for testing connectivity. Any rewards would go to someone else!

## Security Best Practices

### ✅ DO:
- Write down your seed phrase on paper
- Store seed phrase in a safe place
- Use a strong password
- Keep software updated
- Test with small amounts first

### ❌ DON'T:
- Share your seed phrase with anyone
- Store seed phrase digitally (no photos, no cloud)
- Use exchange addresses for mining
- Reuse addresses (use new address each time)
- Keep large amounts in hot wallets

## Address Validation

Before mining, verify your address:

1. **Check Format**
   - Legacy: Starts with `1` or `3`
   - SegWit: Starts with `bc1`
   - Testnet: Starts with `tb1`, `m`, or `n`

2. **Validate Online**
   - Visit: https://www.blockchain.com/explorer
   - Paste your address
   - Should show "Valid address"

3. **Test with Small Amount**
   - Send $1 worth of Bitcoin to your address
   - Verify you can see it in your wallet
   - Confirms address works correctly

## Recommended Setup for BMiner

### For Testing (No Risk):
1. **Use Electrum** (free, easy, secure)
2. **Create testnet wallet** (no real money)
3. **Test BMiner** with testnet address
4. **Verify everything works**

### For Real Mining:
1. **Use Electrum or Hardware Wallet**
2. **Create mainnet wallet**
3. **Backup seed phrase** (write on paper!)
4. **Use address in BMiner**
5. **Monitor in pool dashboard**

## FAQ

**Q: Do I need a wallet to test BMiner?**
A: Technically no, but you won't receive any rewards. Use a demo address for connectivity testing.

**Q: Can I use the same address for multiple miners?**
A: Yes, but it's better to use different addresses for privacy.

**Q: What if I lose my seed phrase?**
A: You lose access to your Bitcoin forever. Always backup!

**Q: How long until I receive mining rewards?**
A: For solo mining, only if you find a block (extremely rare). For pool mining, depends on pool payout schedule.

**Q: Can I change my address later?**
A: Yes, just update the config file and restart BMiner.

## Next Steps

After getting your Bitcoin address:

1. ✅ Copy your address
2. ✅ Copy `config/braiins-pool-test.toml.example` to `config/braiins-pool-test.toml`
3. ✅ Edit `config/braiins-pool-test.toml`
4. ✅ Replace `YOUR_BITCOIN_ADDRESS_HERE`
5. ✅ Save the file
6. ✅ Start BMiner!

```bash
# Create and edit local config
cp config/braiins-pool-test.toml.example config/braiins-pool-test.toml
nano config/braiins-pool-test.toml

# Start mining
./target/release/bminer --config config/braiins-pool-test.toml
```

## Resources

- **Electrum**: https://electrum.org/
- **BlueWallet**: https://bluewallet.io/
- **Bitcoin.org**: https://bitcoin.org/en/choose-your-wallet
- **Address Validator**: https://www.blockchain.com/explorer
- **Testnet Faucet**: https://testnet-faucet.mempool.co/

---

**Remember**: Your seed phrase = your Bitcoin. Keep it safe! 🔐
