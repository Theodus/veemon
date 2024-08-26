use alloy_rlp::Encodable;
use ethers::prelude::*;
use reth::primitives::{Bloom, Log, Receipt, ReceiptWithBloom, TxType, B256};
use reth_trie::{root::adjust_index_for_rlp, HashBuilder, Nibbles};
use serde::{Deserialize, Deserializer, Serialize};

// reth::primitives::proofs::calculate_receipt_root;

#[derive(Debug, Deserialize, Serialize)]
pub struct ReceiptJson {
    #[serde(rename = "type")]
    #[serde(deserialize_with = "str_to_type")]
    pub tx_type: TxType,
    #[serde(rename = "blockHash")]
    pub block_hash: String,
    #[serde(rename = "blockNumber")]
    pub block_number: String,
    pub logs: Vec<Log>,
    #[serde(rename = "cumulativeGasUsed")]
    pub cumulative_gas_used: U256,
    #[serde(deserialize_with = "status_to_bool")]
    pub status: bool,
    // TODO: should we trust logsBloom provided or calculate it from the logs?
    #[serde(rename = "logsBloom")]
    pub logs_bloom: Bloom,
}

impl TryFrom<&ReceiptJson> for ReceiptWithBloom {
    type Error = String;

    fn try_from(receipt_json: &ReceiptJson) -> Result<Self, Self::Error> {
        let cumulative_gas_used = receipt_json
            .cumulative_gas_used
            .try_into()
            .map_err(|_| "Failed to convert U256 to u64".to_string())?;

        let receipt = Receipt {
            tx_type: receipt_json.tx_type,
            success: receipt_json.status,
            cumulative_gas_used,
            logs: receipt_json.logs.clone(),
            // #[cfg(feature = "optimism")]
            // deposit_nonce: None, // Handle Optimism-specific fields as necessary
            // #[cfg(feature = "optimism")]
            // deposit_receipt_version: None,
        };

        // Create the ReceiptWithBloom struct
        Ok(ReceiptWithBloom {
            bloom: receipt_json.logs_bloom,
            receipt,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ReceiptsFromBlock {
    pub result: Vec<ReceiptJson>,
}

fn status_to_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let status_str: &str = Deserialize::deserialize(deserializer)?;
    match status_str {
        "0x1" => Ok(true),
        "0x0" => Ok(false),
        _ => Err(serde::de::Error::custom("Invalid status value")),
    }
}

fn str_to_type<'de, D>(deserializer: D) -> Result<TxType, D::Error>
where
    D: Deserializer<'de>,
{
    let tx_type_str: &str = Deserialize::deserialize(deserializer)?;
    // Convert the hex string (without the "0x" prefix) to u8
    let tx_type_value = u8::from_str_radix(tx_type_str.trim_start_matches("0x"), 16)
        .map_err(|_| serde::de::Error::custom("Invalid tx_type value"))?;
    TxType::try_from(tx_type_value).map_err(|_| serde::de::Error::custom("Invalid tx_type value"))
}

pub fn hash_builder_root(receipts: &[ReceiptWithBloom]) -> B256 {
    let mut index_buffer = Vec::new();
    let mut value_buffer = Vec::new();

    let mut hb = HashBuilder::default();
    let receipts_len = receipts.len();
    for i in 0..receipts_len {
        let index = adjust_index_for_rlp(i, receipts_len);

        index_buffer.clear();
        index.encode(&mut index_buffer);

        value_buffer.clear();
        receipts[index].encode_inner(&mut value_buffer, false);
        hb.add_leaf(Nibbles::unpack(&index_buffer), &value_buffer);
    }

    hb.root()
}

#[cfg(test)]
mod tests {

    use crate::beacon_block::BlockWrapper;
    use std::cell::LazyCell;

    use super::*;

    /// Deneb block JSON file shared among contributors.
    /// The block hash is `0x5dde05ab1da7f768ed3ea2d53c6fa0d79c0c2283e52bb0d00842a4bdbf14c0ab`.
    const DENEB_BLOCK_JSON: &str = include_str!("../../../bb-8786333.json");
    const BLOCK_RECEIPTS_JSON: &str = include_str!("../../../eb-19584570-receipts.json");

    const BLOCK_WRAPPER: LazyCell<BlockWrapper> = LazyCell::new(|| {
        serde_json::from_str(DENEB_BLOCK_JSON).expect(
            "For this spike we are using a Deneb block JSON file that has been shared among contributors",
        )
    });

    const BLOCK_RECEIPTS: LazyCell<ReceiptsFromBlock> = LazyCell::new(|| {
        serde_json::from_str(BLOCK_RECEIPTS_JSON).expect(
            "This is all the receipt data from a block, fetch with eth_getBlockReceipts method",
        )
    });

    #[test]
    fn test_parse_wrapped_receipt_into_reth_receipt() {
        let block_receipts: &LazyCell<ReceiptsFromBlock> = &BLOCK_RECEIPTS;
        let receipts_with_bloom: Result<Vec<ReceiptWithBloom>, String> = block_receipts
            .result
            .iter()
            .map(|receipt_json| ReceiptWithBloom::try_from(receipt_json))
            .collect::<Result<Vec<_>, _>>();

        // Check that the conversion was successful
        assert!(
            receipts_with_bloom.is_ok(),
            "Conversion failed with error: {:?}",
            receipts_with_bloom.err().unwrap()
        );
    }

    #[test]
    fn test_compute_receipts_trie() {
        let block_wrapper: &LazyCell<BlockWrapper> = &BLOCK_WRAPPER;
        let block: &::types::BeaconBlock<::types::MainnetEthSpec> = &block_wrapper.data.message;
        let block_body: &::types::BeaconBlockBodyDeneb<::types::MainnetEthSpec> =
            block.body_deneb().unwrap();
        let payload = &block_body.execution_payload;
        let receipts_root = payload.execution_payload.receipts_root;

        let block_receipts: &LazyCell<ReceiptsFromBlock> = &BLOCK_RECEIPTS;
        let receipts_with_bloom: Result<Vec<ReceiptWithBloom>, String> = block_receipts
            .result
            .iter()
            .map(|receipt_json| ReceiptWithBloom::try_from(receipt_json))
            .collect::<Result<Vec<_>, _>>();

        match receipts_with_bloom {
            Ok(receipts) => {
                let root: reth::revm::primitives::FixedBytes<32> = hash_builder_root(&receipts);
                let root_h256 = H256::from(root.0);
                assert_eq!(root_h256, receipts_root, "Roots do not match!");
            }
            Err(e) => {
                // Handle the error (e.g., by logging or panicking)
                panic!("Failed to convert receipts: {}", e);
            }
        }
    }
}
