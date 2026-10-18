// RUST `use`: brings a tool into this file so it can be used by its short
// name. `::` (no space) means "inside of": std::fs = fs inside std.
// `std` is Rust's standard library; `fs` = file system (reading files).
use std::fs;
use argent_runtime::{
    Artifact, EntryCall, TxBuilder, TxContext, args, state,
};
use kaspa_consensus_core::Hash;
use kaspa_consensus_core::hashing::sighash::{
    SigHashReusedValuesUnsync, calc_schnorr_signature_hash,
};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::tx::{
    CovenantBinding, MutableTransaction, ScriptPublicKey, Transaction,
    TransactionId, TransactionOutpoint, UtxoEntry,
};
// ⚠️ NOT SAFE FOR REAL USE: OpTrue is a script that always says "yes".
// A box locked only with OpTrue can be spent by ANYONE, with no signature.
// It is used here ONLY for the local demo's pretend funding box. In a real
// (testnet) app, fund transactions from your own key-locked wallet coins.
use kaspa_txscript::opcodes::codes::OpTrue;
use secp256k1::{
    Keypair, Message, Secp256k1, SecretKey,
};

// RUST `fn name(param: Type) -> ReturnType { ... }` defines a function.
// Rust parameters are NAME first, then `:`, then TYPE (Argent is the opposite).
// `u8` = a whole number from 0 to 255 (one byte).
// ⚠️ NOT SAFE FOR REAL KEYS: this builds a private key from one repeated
// byte (e.g. 32 copies of 1), which anyone can guess. Local demo only.
fn keypair(byte: u8) -> Keypair {
    // RUST `let name = value;` makes a new variable (Rust works out its type).
    // `Type::new()` creates a fresh one; `()` means "call it now".
    let secp = Secp256k1::new();
    // `[byte; 32]` = byte repeated 32 times (inside [ ], `;` means "times").
    // `&` = lend it (the function only looks at it).
    // `.expect("...")` = if this failed, stop the program with this message.
    let secret = SecretKey::from_slice(&[byte; 32]).expect("valid secret key");
    // RUST: the last line WITHOUT `;` is the value the function gives back.
    Keypair::from_secret_key(&secp, &secret)
}

// `&Keypair` = the key pair is lent (only looked at).
// `Vec<u8>` = a list of bytes of any length ("vector").
fn public_key(keys: &Keypair) -> Vec<u8> {
    // A chain: each `.` does one more step. `.0` = first item of a pair.
    // Kaspa uses the 32-byte "x-only" public key. No `;` = returned value.
    keys.x_only_public_key().0.serialize().to_vec()
}

// Makes the same 32-byte blake2b fingerprint the contract computes, so the
// stored admin/owner values match. `&[u8]` = any number of bytes, lent.
fn fingerprint(data: &[u8]) -> Vec<u8> {
    blake2b_simd::Params::new().hash_length(32).hash(data).as_bytes().to_vec()
}

// Signs the transaction for one input box, the same way the repo's tests do
// (sign_mutable_input). Produces the `sig` value the contract's checkSig tests.
fn sign(
    // `< >` fills in a detail of a type: a MutableTransaction wrapping a
    // Transaction. The runtime hands the transaction over in exactly this type.
    tx: &MutableTransaction<Transaction>,
    // `usize` = a position or count (never negative). Input 0 = first box.
    input_index: usize,
    keys: &Keypair,
) -> Vec<u8> {
    let reused = SigHashReusedValuesUnsync::new();
    let hash = calc_schnorr_signature_hash(&tx.as_verifiable(), input_index, SIG_HASH_ALL, &reused);
    let message = Message::from_digest_slice(hash.as_bytes().as_slice()).expect("32-byte message");
    // RUST `mut` = this variable may be changed later (the next line adds to it).
    let mut signature = keys.sign_schnorr(message).as_ref().to_vec();
    // Label byte: tells checkSig this signature covers ALL of the transaction.
    signature.push(SIG_HASH_ALL.to_u8());
    signature
}

// Prints whether Kaspa's script engine accepted or rejected a transaction.
// `Result` = either Ok (success) or Err (failure, with the reason).
fn report(step: &str, result: Result<Transaction, argent_runtime::BuilderError>) {
    match result {
        Ok(tx) => println!("ACCEPTED  {step}  (tx id {})", tx.id()),
        Err(error) => println!("REJECTED  {step}\n          reason: {error}"),
    }
}

fn main() {
    // Load YOUR compiled GigTickets contract (run `argentc build` first).
    let json = fs::read_to_string("../build/artifact.json").expect("run argentc build first");
    let artifact: Artifact = serde_json::from_str(&json).expect("valid artifact");
    let builder = TxBuilder::new(&artifact).expect("builder accepts artifact");

    // Three people. ⚠️ Guessable demo keys: local testing only.
    let admin = keypair(1);
    let buyer = keypair(2);
    let stranger = keypair(3);

    // ⚠️ Made-up covenant ID. On a real network Kaspa calculates it when
    // the app is launched (genesis). Fine here: nothing leaves this computer.
    let covenant_id = Hash::from_bytes([9; 32]);

    // Box data, matching your MoxyState and TicketState field names exactly.
    let issuer_before = state! {
        admin: fingerprint(&public_key(&admin)),
        nextTicket: 0,
    };
    let issuer_after = state! {
        admin: fingerprint(&public_key(&admin)),
        nextTicket: 1,
    };
    let ticket_unused = state! {
        owner: fingerprint(&public_key(&buyer)),
        serial: 0,
        redeemed: 0,
    };
    let ticket_used = state! {
        owner: fingerprint(&public_key(&buyer)),
        serial: 0,
        redeemed: 1,
    };

    // The existing Issuer box (1000 coins) and a pretend funding box (600).
    let issuer_utxo = builder
        .covenant_utxo("Issuer", issuer_before.clone(), 1_000, 0, false, Some(covenant_id))
        .expect("issuer box builds");
    let issuer_outpoint = TransactionOutpoint::new(TransactionId::from_bytes([1; 32]), 0);
    // ⚠️ OpTrue funding box: anyone could spend this. Local demo only.
    let funding_utxo = UtxoEntry::new(600, ScriptPublicKey::new(0, vec![OpTrue].into()), 0, false, None);
    let funding_outpoint = TransactionOutpoint::new(TransactionId::from_bytes([2; 32]), 0);

    // Builds an `issue` transaction signed by `signer`.
    // In: Issuer box + funding box. Out: updatedIssuer (1000) + newTicket (500).
    let issue_by = |signer: &Keypair| {
        let signer = *signer;
        TxContext::new()
            .actor_input(
                "Issuer",
                issuer_before.clone(),
                EntryCall::new("issue")
                    .args_with(move |tx, i| args![sign(tx, i, &signer), public_key(&signer), public_key(&buyer)]),
                issuer_outpoint,
                issuer_utxo.clone(),
                0,
            )
            .input(funding_outpoint, funding_utxo.clone(), Vec::new(), 0)
            .actor_output("Issuer", issuer_after.clone(), CovenantBinding::new(0, covenant_id), 1_000)
            .actor_output("Ticket", ticket_unused.clone(), CovenantBinding::new(0, covenant_id), 500)
    };

    report("1. admin issues ticket #0 to the buyer", builder.build(&issue_by(&admin)));
    report("2. stranger tries to issue a ticket", builder.build(&issue_by(&stranger)));

    // Builds a `redeem` transaction for a ticket box, signed by `signer`.
    let redeem = |ticket_state: &std::collections::BTreeMap<String, argent_runtime::ArtifactValue>, signer: &Keypair| {
        let signer = *signer;
        let ticket_utxo = builder
            .covenant_utxo("Ticket", ticket_state.clone(), 500, 0, false, Some(covenant_id))
            .expect("ticket box builds");
        TxContext::new()
            .actor_input(
                "Ticket",
                ticket_state.clone(),
                EntryCall::new("redeem").args_with(move |tx, i| args![sign(tx, i, &signer), public_key(&signer)]),
                TransactionOutpoint::new(TransactionId::from_bytes([3; 32]), 1),
                ticket_utxo,
                0,
            )
            .actor_output("Ticket", ticket_used.clone(), CovenantBinding::new(0, covenant_id), 500)
    };

    report("3. buyer redeems ticket #0", builder.build(&redeem(&ticket_unused, &buyer)));
    report("4. stranger tries to redeem the buyer's ticket", builder.build(&redeem(&ticket_unused, &stranger)));
    report("5. buyer tries to redeem the same ticket again", builder.build(&redeem(&ticket_used, &buyer)));
}
