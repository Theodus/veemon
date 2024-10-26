use super::{Block, BlockHeader};
use alloy_primitives::{Address, Bloom, FixedBytes, Uint};
use ethportal_api::types::execution::header::Header;
use prost::Message;
use prost_wkt_types::Any;
use reth_primitives::{proofs::calculate_transaction_root, TransactionSigned};
use tracing::error;

use crate::{
    error::ProtosError,
    firehose::v2::{Response, SingleBlockResponse},
};

impl TryFrom<&Block> for Header {
    type Error = ProtosError;

    fn try_from(block: &Block) -> Result<Self, Self::Error> {
        let block_header = block
            .header
            .as_ref()
            .ok_or(ProtosError::BlockConversionError)?;

        let parent_hash = FixedBytes::from_slice(block_header.parent_hash.as_slice());
        let uncles_hash = FixedBytes::from_slice(block_header.uncle_hash.as_slice());
        let author = Address::from_slice(block_header.coinbase.as_slice());
        let state_root = FixedBytes::from_slice(block_header.state_root.as_slice());
        let transactions_root = FixedBytes::from_slice(block_header.transactions_root.as_slice());
        let receipts_root = FixedBytes::from_slice(block_header.receipt_root.as_slice());
        let logs_bloom = Bloom::from_slice(block_header.logs_bloom.as_slice());
        let difficulty = Uint::from_be_slice(
            block_header
                .difficulty
                .as_ref()
                .ok_or(ProtosError::BlockConversionError)?
                .bytes
                .as_slice(),
        );
        let number = block_header.number;
        let gas_limit = Uint::from(block_header.gas_limit);
        let gas_used = Uint::from(block_header.gas_used);
        let timestamp = block_header
            .timestamp
            .as_ref()
            .ok_or(ProtosError::BlockConversionError)?
            .seconds as u64;
        let extra_data = block_header.extra_data.clone();
        let mix_hash = Some(FixedBytes::from_slice(block_header.mix_hash.as_slice()));
        let nonce = Some(FixedBytes::from_slice(&block_header.nonce.to_be_bytes()));
        let base_fee_per_gas = block_header
            .base_fee_per_gas
            .as_ref()
            .map(|base_fee_per_gas| Uint::from_be_slice(base_fee_per_gas.bytes.as_slice()));
        let withdrawals_root = match block_header.withdrawals_root.is_empty() {
            true => None,
            false => Some(FixedBytes::from_slice(
                block_header.withdrawals_root.as_slice(),
            )),
        };
        let blob_gas_used = block_header.blob_gas_used.map(Uint::from);
        let excess_blob_gas = block_header.excess_blob_gas.map(Uint::from);
        let parent_beacon_block_root = match block_header.parent_beacon_root.is_empty() {
            true => None,
            false => Some(FixedBytes::from_slice(
                block_header.parent_beacon_root.as_slice(),
            )),
        };

        Ok(Header {
            parent_hash,
            uncles_hash,
            author,
            state_root,
            transactions_root,
            receipts_root,
            logs_bloom,
            difficulty,
            number,
            gas_limit,
            gas_used,
            timestamp,
            extra_data,
            mix_hash,
            nonce,
            base_fee_per_gas,
            withdrawals_root,
            blob_gas_used,
            excess_blob_gas,
            parent_beacon_block_root,
        })
    }
}

fn decode_block<M>(response: M) -> Result<Block, ProtosError>
where
    M: MessageWithBlock,
{
    let any = response.block().ok_or(ProtosError::NullBlock)?;
    let block = Block::decode(any.value.as_ref())?;
    Ok(block)
}

trait MessageWithBlock {
    fn block(&self) -> Option<&Any>;
}

impl MessageWithBlock for SingleBlockResponse {
    fn block(&self) -> Option<&Any> {
        self.block.as_ref()
    }
}

impl MessageWithBlock for Response {
    fn block(&self) -> Option<&Any> {
        self.block.as_ref()
    }
}

impl TryFrom<SingleBlockResponse> for Block {
    type Error = ProtosError;

    fn try_from(response: SingleBlockResponse) -> Result<Self, Self::Error> {
        decode_block(response)
    }
}

impl TryFrom<Response> for Block {
    type Error = ProtosError;

    fn try_from(response: Response) -> Result<Self, Self::Error> {
        decode_block(response)
    }
}

impl Block {
    fn calculate_transaction_root(&self) -> Result<FixedBytes<32>, ProtosError> {
        let transactions = self.transaction_traces_to_signed_transactions()?;
        Ok(calculate_transaction_root(&transactions))
    }

    fn header(&self) -> Result<&BlockHeader, ProtosError> {
        self.header.as_ref().ok_or(ProtosError::MissingBlockHeader)
    }

    fn transaction_traces_to_signed_transactions(
        &self,
    ) -> Result<Vec<TransactionSigned>, ProtosError> {
        self.transaction_traces
            .iter()
            .map(|trace| trace.try_into())
            .collect()
    }

    /// Checks if the transaction root matches the block header's transactions root.
    /// Returns `true` if they match, `false` otherwise.
    pub fn transaction_root_is_verified(&self) -> bool {
        let tx_root = match self.calculate_transaction_root() {
            Ok(tx_root) => tx_root,
            Err(e) => {
                error!("Failed to calculate transaction root: {e}");
                return false;
            }
        };

        match self.verify_transaction_root(tx_root.as_slice()) {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to verify transaction root: {e}");
                false
            }
        }
    }

    fn verify_transaction_root(&self, other_transaction_root: &[u8]) -> Result<bool, ProtosError> {
        Ok(other_transaction_root == self.header()?.transactions_root.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use ethportal_api::Header;

    use crate::ethereum_v2::BlockHeader;

    use super::*;

    #[test]
    fn test_block_to_header() {
        let block_header: BlockHeader = serde_json::from_str(BLOCK).unwrap();

        // Confirm block hash.
        assert_eq!(
            format!("0x{}", hex::encode(&block_header.hash)).as_str(),
            "0xf218f8b4f7879b1c4a44b658a32d4a338db85c85c2916229d8b1c7728b448382"
        );

        let block = Block {
            header: Some(block_header),
            ..Default::default()
        };

        let header = Header::try_from(&block).unwrap();

        // Calculate the block hash from the header.
        // `hash()` calls `keccak256(alloy_rlp::encode(self))`.
        let block_hash = header.hash();

        assert_eq!(
            block_hash.to_string().as_str(),
            "0xf218f8b4f7879b1c4a44b658a32d4a338db85c85c2916229d8b1c7728b448382"
        );
    }

    static BLOCK: &str = r###"
        {
            "parent_hash":[41,204,132,204,44,220,150,185,95,11,250,60,105,128,80,38,218,105,225,93,10,199,246,153,65,41,143,174,97,80,153,227],
            "uncle_hash":[29,204,77,232,222,199,93,122,171,133,181,103,182,204,212,26,211,18,69,27,148,138,116,19,240,161,66,253,64,212,147,71],
            "coinbase":[149,34,34,144,221,114,120,170,61,221,56,156,193,225,209,101,204,75,175,229],
            "state_root":[189,117,186,190,39,215,6,165,69,5,75,43,173,63,205,229,186,255,252,204,249,187,167,135,42,184,106,76,115,135,183,196],
            "transactions_root":[91,168,44,68,170,165,170,154,91,187,142,155,122,30,110,32,165,97,67,168,82,249,207,207,149,219,133,234,130,117,47,123],
            "receipt_root":[145,75,161,249,110,54,93,87,143,233,225,142,38,45,186,255,155,29,17,244,90,31,177,92,248,49,53,212,53,175,250,173],
            "logs_bloom":[149,189,130,99,255,12,121,222,183,233,18,212,226,89,227,240,177,142,207,218,159,207,119,7,202,201,132,158,254,250,231,238,128,157,207,188,252,33,219,42,218,0,91,57,131,191,221,203,159,243,142,254,238,82,234,251,243,222,127,142,247,191,57,250,183,91,95,249,2,233,251,123,238,62,197,125,189,201,178,160,161,245,255,167,105,86,234,242,125,234,252,229,222,236,203,124,174,30,241,217,207,251,67,126,55,127,254,254,93,77,62,235,254,114,198,123,249,157,191,253,199,106,211,215,234,255,248,239,170,163,150,120,155,75,11,95,136,255,247,246,189,243,96,183,15,90,243,67,251,237,184,238,254,251,245,122,115,127,127,187,223,254,121,34,31,183,227,143,95,220,93,214,250,26,63,14,54,215,53,140,148,251,240,95,175,127,205,183,182,43,139,117,251,152,148,38,229,182,255,93,49,120,246,235,73,187,251,180,75,246,246,255,247,60,191,120,233,71,251,22,97,190,107,149,218,125,250,94,151,212,31,226,145,157,254,147,44,233,220,230,31,253,246,34,123,250,235,210,178,175,146,115,218,199,247,231],
            "difficulty":{"bytes":[0]},
            "total_difficulty":{"bytes":[12,112,216,21,213,98,211,207,169,85]},
            "number":20562650,
            "gas_limit":30000000,
            "gas_used":21017587,
            "timestamp":"2024-08-19T12:23:23Z",
            "extra_data":[98,101,97,118,101,114,98,117,105,108,100,46,111,114,103],
            "mix_hash":[252,10,116,218,224,219,162,6,51,85,19,59,234,116,27,166,142,92,116,59,194,160,194,122,92,69,160,127,217,173,205,24],
            "nonce":0,
            "hash":[242,24,248,180,247,135,155,28,74,68,182,88,163,45,74,51,141,184,92,133,194,145,98,41,216,177,199,114,139,68,131,130],
            "base_fee_per_gas":{"bytes":[98,32,239,15]},
            "withdrawals_root":[43,236,160,133,139,4,30,79,53,95,69,56,245,209,1,32,174,121,3,234,213,71,185,39,252,76,182,2,128,212,199,94],
            "tx_dependency":null,
            "blob_gas_used":131072,
            "excess_blob_gas":0,
            "parent_beacon_root":[200,178,112,247,15,219,223,40,221,158,56,205,13,155,9,68,32,137,201,81,195,111,239,86,19,255,147,198,140,203,232,34]
        }
    "###;
}
