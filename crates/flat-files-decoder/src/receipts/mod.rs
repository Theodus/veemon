pub mod error;
mod logs;
pub(crate) mod receipt;

use crate::protos::block::Block;
use crate::receipts::error::ReceiptError;
use reth_blockchain_tree::post_state::PostState;
use reth_primitives::{hex, Receipt};

pub fn check_receipt_root(block: &Block) -> Result<(), ReceiptError> {
    let mut post_state = PostState::new();

    for trace in &block.transaction_traces {
        post_state.add_receipt(block.number, Receipt::try_from(trace)?);
    }

    let computed_root = post_state.receipts_root(block.number);

    if computed_root.as_bytes() != block.header.receipt_root.as_slice() {
        return Err(ReceiptError::MismatchedRoot(
            hex::encode(computed_root.as_bytes()),
            hex::encode(block.header.receipt_root.as_slice()),
        ));
    }

    Ok(())
}
