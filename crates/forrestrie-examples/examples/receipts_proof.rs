//! # Receipts proof given an EL block's `receipt_root``
//!
//! This example shows how to generate an inclusion proof for a set of receipts of a EL block;
//!
use firehose_client::client::{Chain, FirehoseClient};
use firehose_protos::ethereum_v2::{self, eth_block::FullReceipt, Block};
use forrestrie::execution_layer::{build_trie_with_proofs, TargetLeaves};
use reth_primitives::ReceiptWithBloom;
use reth_trie_common::proof::verify_proof;

const EXECUTION_BLOCK_NUMBER: u64 = 20759937;

#[tokio::main]
async fn main() {
    let mut eth1_client = FirehoseClient::new(Chain::Ethereum);
    let response = eth1_client
        .fetch_block(EXECUTION_BLOCK_NUMBER)
        .await
        .unwrap()
        .unwrap();
    let eth1_block: Block = ethereum_v2::Block::try_from(response.into_inner()).unwrap();

    let receipts: Vec<FullReceipt> = eth1_block
        .transaction_traces
        .iter()
        .filter_map(|trace| {
            // Attempt to convert each trace into a FullReceipt
            FullReceipt::try_from(trace).ok() // Collect only successful conversions
        })
        .collect();

    let receipts_with_bloom: Vec<ReceiptWithBloom> = receipts
        .iter()
        .map(|full_receipt| full_receipt.receipt.clone())
        .collect();

    // These are de indexes of receipts on which proofs have to be generated
    let target_idxs = &[1, 2, 3];
    let targets = TargetLeaves::from_indices(target_idxs, &receipts_with_bloom).unwrap();
    let mut hb = build_trie_with_proofs(&receipts_with_bloom, target_idxs);

    // produces the root, which matches the root of the blocks.
    // hb.root() also calculates the proofs and store them in the HashBuilder.
    let root = hb.root();

    let calc_root = eth1_block.calculate_receipt_root();
    println!("roots: {:?},  {:?}", root, calc_root);

    // proofs can be taken and sorted, so each proof matches one of the target.
    // each proof of a specific target receipt is provided in `take_proof_nodes()`
    // and can be stored or verified singularly
    let proof = hb.take_proof_nodes();
    for target in targets {
        let _verification = verify_proof(
            hb.root(),
            target.nibbles.clone(),
            Some(target.value.to_vec()),
            proof
                .clone()
                .matching_nodes_sorted(&target.nibbles)
                .iter()
                .map(|(_, node)| node),
        );
    }
}
