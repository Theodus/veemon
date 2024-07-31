use primitive_types::H256;
use serde::{Deserialize, Serialize};
use tree_hash::TreeHash;
use types::{BeaconState, Error, EthSpec, MainnetEthSpec};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HeadState<E: EthSpec> {
    version: String,
    execution_optimistic: bool,
    data: BeaconState<E>,
}

impl HeadState<MainnetEthSpec> {
    pub fn compute_merkle_proof(&self, index: usize) -> Result<Vec<H256>, Error> {
        self.data.compute_merkle_proof(index)
    }

    pub fn data(&self) -> &BeaconState<MainnetEthSpec> {
        &self.data
    }

    pub fn execution_optimistic(&self) -> bool {
        self.execution_optimistic
    }

    pub fn historical_roots_tree_hash_root(&self) -> H256 {
        self.data.historical_roots().tree_hash_root()
    }

    pub fn historical_summaries_tree_hash_root(&self) -> Result<H256, Error> {
        Ok(self.data.historical_summaries()?.tree_hash_root())
    }

    pub fn state_root(&self) -> H256 {
        self.data.tree_hash_root()
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use lazy_static::lazy_static;
    use merkle_proof::verify_merkle_proof;
    use types::light_client_update::{
        CURRENT_SYNC_COMMITTEE_PROOF_LEN, HISTORICAL_ROOTS_INDEX, HISTORICAL_SUMMARIES_INDEX,
    };

    const HEAD_STATE_JSON: &str = include_str!("../head-state.json");
    const HISTORICAL_ROOTS_FIELD_INDEX: usize = 7;
    const HISTORICAL_SUMMARIES_FIELD_INDEX: usize = 27;

    lazy_static! {
        static ref STATE: HeadState<MainnetEthSpec> = serde_json::from_str(HEAD_STATE_JSON).expect(
            "For this spike we are using a 'head-state.json' file that has been shared among contributors"
        );
    }

    #[test]
    fn test_inclusion_proofs_with_historical_and_state_roots() {
        let state = &STATE;

        let proof = state.compute_merkle_proof(HISTORICAL_ROOTS_INDEX).unwrap();

        insta::assert_debug_snapshot!(proof, @r###"
        [
            0xe81a79506c46b126f75a08cdd5cbc35052b61ca944c6c3becf32432e2ee6373a,
            0xcfb49cd7eb0051153685e5e6124b635c6b9bcc69a6ead6af0ef7d9885fcc16e2,
            0x29c2e1f6d96493e9b49517cb78123990038429e4c3574688a48f9abe69238449,
            0xdb329a01d9114f087155633b36b498c8e60028c0acedc8e3b64e013dbbd4fa06,
            0x53b107024e402f616f8f348d900e0d62f4b6f0558d2bfbd09200e68620a5b9c2,
        ]
        "###);

        let historical_roots_tree_hash_root = state.historical_roots_tree_hash_root();

        let state_root = state.state_root();

        let depth = CURRENT_SYNC_COMMITTEE_PROOF_LEN;

        assert!(
            verify_merkle_proof(
                historical_roots_tree_hash_root,
                &proof,
                depth,
                HISTORICAL_ROOTS_FIELD_INDEX,
                state_root
            ),
            "Merkle proof verification failed"
        );
    }

    #[test]
    fn test_inclusion_proofs_for_historical_summary_given_historical_summaries_root() {
        let state = &STATE;

        let proof = state
            .compute_merkle_proof(HISTORICAL_SUMMARIES_INDEX)
            .unwrap();

        insta::assert_debug_snapshot!(proof, @r###"
        [
            0x053a090000000000000000000000000000000000000000000000000000000000,
            0x455a0d1e0a3b5660d74b6520062c9c3cead986928686e535451ca6e61aeb291f,
            0xdb56114e00fdd4c1f85c892bf35ac9a89289aaecb1ebd0a96cde606a748b5d71,
            0xc204e43766c4e9d43da1a54c3053024eef28d407bcca7936900ffd2e7aa165b2,
            0x2150a88f205759c59817f42dc307620c67d3d23417959286928d186c639a0948,
        ]
        "###);

        let historical_summaries_tree_hash_root =
            state.historical_summaries_tree_hash_root().unwrap();

        let state_root = state.state_root();

        let depth = CURRENT_SYNC_COMMITTEE_PROOF_LEN;

        assert!(
            verify_merkle_proof(
                historical_summaries_tree_hash_root,
                &proof,
                depth,
                HISTORICAL_SUMMARIES_FIELD_INDEX,
                state_root
            ),
            "Merkle proof verification failed"
        );
    }
}
