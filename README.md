# Rain Oracle Server

Reference implementation of a signed context oracle server for [Raindex](https://rainlang.xyz) orders.

Serves `SignedContextV1` data that Rain orderbook takers can use when taking orders that require external data (e.g. price feeds).

## How it works

1. Fetches ETH/USD price from [Pyth Hermes API](https://hermes.pyth.network) (free, no API key)
2. Scales price to 18 decimal fixed point
3. Builds a context array: `[price_18_decimals, expiry_timestamp]`
4. Signs via EIP-191: `sign(keccak256(abi.encodePacked(context[])))`
5. Returns `{ signer, context, signature }` matching Rain's `SignedContextV1`

## Usage

```bash
# Set your signer private key
export SIGNER_PRIVATE_KEY=0x...

# Run
cargo run

# Or with nix
nix develop -c cargo run
```

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SIGNER_PRIVATE_KEY` | (required) | Hex private key for EIP-191 signing |
| `PORT` | `3000` | Server port |
| `PYTH_PRICE_FEED_ID` | ETH/USD | Pyth feed ID (hex without 0x) |
| `EXPIRY_SECONDS` | `5` | Signed context expiry in seconds |

### Endpoint

```
GET /context
```

Response:
```json
{
  "signer": "0x...",
  "context": ["0x...", "0x..."],
  "signature": "0x..."
}
```

Context layout (all values are Rain DecimalFloats):
- `context[0]`: ETH/USD price as Rain float (Pyth coefficient * 10^exponent, packed directly)
- `context[1]`: expiry timestamp as Rain float (unix seconds, `now + EXPIRY_SECONDS`, exponent=0)

## Rainlang usage

In your order expression, validate the signed context:

```
expiry: signed-context<0 1>(),
:ensure(greater-than(expiry block-timestamp())),
eth-price: signed-context<0 0>();
```

## Development

```bash
nix develop    # enter dev shell
cargo test     # run tests
cargo clippy   # lint
cargo fmt      # format
```

## License

MIT
