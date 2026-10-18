# GigTickets: an Argent learning project

A hands-on walkthrough of the Argent language, built by retyping the repo's
[`examples/tickets.ag`](../examples/tickets.ag) line by line with new names,
then test-driving the compiled contract with a small local Rust program.

> **⚠️ Local learning code only. Do not use on mainnet, and do not reuse its
> keys or funding pattern anywhere real.** See [Caveats](#caveats).

## What's here

| Path | What it is |
| --- | --- |
| [`moxy.ag`](moxy.ag) | The GigTickets contract: an `Issuer` actor that issues tickets and a `Ticket` actor that can be redeemed once by its owner. It's annotated with `KEYWORD` and `TYPE` notes. |
| [`demo/`](demo) | A Rust program that loads the compiled contract and runs five transactions through Kaspa's script engine on your own machine. |

`moxy.ag` is the same logic as `examples/tickets.ag`. Its compiled actor sizes
and static opcode counts match the original exactly (Issuer 349 B / 245
opcodes, Ticket 189 B / 123 opcodes).

## Build and run

From the repository root, compile the contract:

```sh
cargo build
./target/debug/argentc build learn/moxy.ag --out learn/build
./target/debug/argentc inspect learn/build
```

Then run the demo from `learn/demo` (the first build takes a few minutes):

```sh
cd learn/demo
cargo run
```

Expected output:

```text
ACCEPTED  1. admin issues ticket #0 to the buyer
REJECTED  2. stranger tries to issue a ticket
ACCEPTED  3. buyer redeems ticket #0
REJECTED  4. stranger tries to redeem the buyer's ticket
REJECTED  5. buyer tries to redeem the same ticket again
```

To see a contract rule doing its job, delete `require (redeemed == 0);` from
`moxy.ag`, rebuild, and run the demo again. Attempt 5 (redeeming twice) is then
**accepted**.

## Caveats

This project is for learning on a single computer. Every shortcut below is
deliberate and unsafe outside that setting.

- **No network at all.** The demo never connects to a Kaspa node. It builds
  transactions and checks them locally with `argent-runtime` and Kaspa's
  script engine. Nothing is broadcast, and no real coins exist.
- **Never deploy to mainnet.** Argent itself is *"not yet release-ready"* (see
  the [project README](../README.md#project-status)), and covenants hold real
  money. Any next step would be Kaspa **testnet** only.
- **Guessable keys.** `keypair(byte)` builds a private key from one repeated
  byte (for example 32 copies of `1`). Anyone can recreate these keys. Never
  use this pattern for real keys.
- **Anyone-can-spend funding box.** The demo funds the new ticket from a box
  locked with only `OpTrue`, which anyone could spend. Real transactions must
  be funded from key-locked wallet coins.
- **Made-up covenant ID.** The demo uses a fixed fake ID (`[9; 32]`). On a real
  network, Kaspa calculates the covenant ID when the app is launched (genesis),
  from the spent outpoint and the initial outputs. See
  [Genesis proof tooling](../docs/genesis-proof-tooling.md).
- **Hand-made signatures.** The demo signs transactions itself, playing the role
  of a wallet. Contracts only *check* signatures (`checkSig`). In a real app, a
  wallet signs, and private keys never leave the owner's device.
- **Not an official example.** The demo is assembled from the public
  `argent-runtime` API and helpers in the repo's own tests
  (`src/builder/tests.rs`). It may need updating if the runtime API changes.
  The official client examples live in
  [argent-playground](https://github.com/argent-lang/argent-playground).
- **The compiler can't catch logic mistakes.** It rejects wrong types, wrong
  argument order, and missing value checks. It can't tell that, for example,
  `owner: admin` gives every ticket to the admin. Both values are `byte[32]`.

## Argent gotchas found along the way

These were each confirmed by running `argentc`:

- Comments are **not** allowed inside a state value's `{ field: value, }` list.
  The compiler reads them as field names. Put the comments above the
  declaration instead.
- `become` must be the last statement in an entry and ends with `;`. A trailing
  `,` gives "`become` must be terminal".
- A local variable can't reuse an `emits` handle name ("collides with emit
  handle"), so this project uses `updatedIssuer` for the box and
  `updatedIssuerData` for its data.
- `checkSig(signature, publicKey)`: the order matters. Swapping the arguments
  fails with "argument 'signature' expects sig, got pubkey".
- Every emitted output's `.value` must be checked, or explicitly marked
  `unrestricted(...)`.
- An app must list every actor its entries emit. Leaving out `actor Ticket;`
  fails with "emits unknown actor `Ticket`".
