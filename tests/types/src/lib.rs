// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Types used by the test-bridge contract.

#![no_std]
#![feature(cfg_eval)]
#![deny(missing_docs)]
#![deny(clippy::pedantic)]
#![allow(clippy::used_underscore_binding)]

extern crate alloc;

use alloc::vec::Vec;

use bytecheck::CheckBytes;
use dusk_bytes::Serializable;
use dusk_core::abi::ContractId;
use dusk_core::signatures::bls::PublicKey;
use rkyv::{Archive, Deserialize, Serialize};

#[cfg(feature = "serde")]
use serde_with::{hex::Hex, serde_as};

// =========================================================================
// Address
// =========================================================================

/// The `DuskDS` address. This can be either a public account or a contract-id.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize,
)]
#[archive_attr(derive(CheckBytes))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Address {
    /// An externally owned public-key.
    External(PublicKey),
    /// A contract-id.
    Contract(ContractId),
}

impl From<PublicKey> for Address {
    fn from(pk: PublicKey) -> Self {
        Self::External(pk)
    }
}

impl From<ContractId> for Address {
    fn from(contract: ContractId) -> Self {
        Self::Contract(contract)
    }
}

impl Address {
    /// Converts the `Address` to a vector of bytes of 193 bytes in case of an
    /// external address, and 32 bytes in case of a contract address.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Address::External(pk) => pk.to_raw_bytes().to_vec(),
            Address::Contract(id) => id.to_bytes().to_vec(),
        }
    }
}

// =========================================================================
// EVMAddress
// =========================================================================

/// Address on `DuskEVM` to bridge to or from.
#[derive(
    Default, Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize,
)]
#[archive_attr(derive(CheckBytes))]
#[cfg_attr(feature = "serde", cfg_eval, serde_as)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EVMAddress(
    #[cfg_attr(feature = "serde", serde(with = "serde_evm"))] pub [u8; 20],
);

// =========================================================================
// SetU64
// =========================================================================

/// The input argument for setting a `u64` contract state variable.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize,
)]
#[archive_attr(derive(CheckBytes))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SetU64 {
    /// Enum variant for setting the `finalization_period` contract state
    /// variable to a new value.
    FinalizationPeriod(u64),
    /// Enum variant for setting the `deposit_fee` contract state variable to a
    /// new value.
    DepositFee(u64),
    /// Enum variant for setting the `deposit_gas_limit` contract state
    /// variable to a new value.
    DepositGasLimit(u64),
    /// Enum variant for setting the `minimum_gas_limit` contract state
    /// variable to a new value.
    MinGasLimit(u64),
    /// Enum variant for setting the `max_data_length` contract state variable
    /// to a new value.
    MaxDataLength(u64),
}

// =========================================================================
// SetEVMAddressOrOffset
// =========================================================================

/// The input argument for setting a `EVMAddress` contract state variable.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize,
)]
#[archive_attr(derive(CheckBytes))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SetEVMAddressOrOffset {
    /// Enum variant for setting the `this_bridge_mapped` contract state
    /// variable to a new value.
    ThisBridgeMapped(EVMAddress),
    /// Enum variant for setting the `this_messenger_mapped` contract state
    /// variable to a new value.
    ThisMessengerMapped(EVMAddress),
    /// Enum variant for setting the `other_bridge` contract state variable to
    /// a new value.
    OtherBridge(EVMAddress),
    /// Enum variant for setting the `other_messenger` contract state variable
    /// to a new value.
    OtherMessenger(EVMAddress),
    /// Enum variant for setting the `alias_offset` contract state variable to
    /// a new value.
    AliasOffset(EVMAddress),
}

// =========================================================================
// Deposit
// =========================================================================

/// The data for calling `deposit` on the `StandardBridge`.
#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes))]
#[cfg_attr(feature = "serde", cfg_eval, serde_as)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Deposit {
    /// Address of the receiver on `DuskEVM`.
    pub to: EVMAddress,
    /// Amount of DUSK sent in Lux.
    pub amount: u64,
    /// Fee for finishing the transaction on `DuskEVM` in Lux.
    pub fee: u64,
    /// Extra data sent with the transaction.
    #[cfg_attr(feature = "serde", serde_as(as = "Hex"))]
    pub extra_data: Vec<u8>,
}

// =========================================================================
// WithdrawalId
// =========================================================================

/// The hashed block-height, event-index and tx-id which uniquely identifies a
/// withdrawal on the L2
#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes))]
#[derive(Copy, Ord, PartialOrd)] // Required for being a BTreeMap key
#[cfg_attr(feature = "serde", cfg_eval, serde_as)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WithdrawalId(
    #[cfg_attr(feature = "serde", serde_as(as = "Hex"))] pub [u8; 32],
);

// =========================================================================
// WithdrawalRequest
// =========================================================================

/// The data for a pending withdrawal on the `StandardBridge` state.
#[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[archive_attr(derive(CheckBytes))]
#[cfg_attr(feature = "serde", cfg_eval, serde_as)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WithdrawalRequest {
    /// The hashed block-height, event-index and tx-id which uniquely
    /// identifies a withdrawal on the L2
    pub id: WithdrawalId,
    /// Address of the sender on `DuskEVM`.
    pub from: EVMAddress,
    /// Amount of DUSK sent in Wei converted to big endian bytes.
    #[cfg_attr(feature = "serde", serde_as(as = "Hex"))]
    pub amount: [u8; 32],
    /// Extra data sent with the L2 transaction, holding the encoded `DuskDS`
    /// `to` address.
    #[cfg_attr(feature = "serde", serde_as(as = "Hex"))]
    pub extra_data: Vec<u8>,
}

impl WithdrawalRequest {
    /// Creates a new `WithdrawalRequest` by prepending the `to` public key to
    /// the `extra_data` field and converting the amount from Lux to Wei.
    #[must_use]
    pub fn new(
        id: WithdrawalId,
        from: EVMAddress,
        to: PublicKey,
        amount: u64,
        extra_data: Vec<u8>,
    ) -> Self {
        let mut extra_data = extra_data;
        let mut new_extra_data = encode_ds_address(to);
        new_extra_data.append(&mut extra_data);
        Self {
            id,
            from,
            amount: {
                let wei = u128::from(amount) * 1_000_000_000;
                let mut bytes = [0u8; 32];
                bytes[16..].copy_from_slice(&wei.to_be_bytes());
                bytes
            },
            extra_data: new_extra_data,
        }
    }
}

impl TryFrom<WithdrawalRequest> for PendingWithdrawal {
    type Error = &'static str;
    fn try_from(withdrawal: WithdrawalRequest) -> Result<Self, Self::Error> {
        let to = decode_ds_address(withdrawal.extra_data)?;

        Ok(PendingWithdrawal {
            from: withdrawal.from,
            to: to.into(),
            #[allow(clippy::cast_possible_truncation)]
            amount: {
                let mut buf = [0u8; 16];
                buf.copy_from_slice(&withdrawal.amount[16..]);
                (u128::from_be_bytes(buf) / 1_000_000_000) as u64
            },
            block_height: u64::MAX,
        })
    }
}

// =========================================================================
// PendingWithdrawal
// =========================================================================

/// The data for a pending withdrawal on the `StandardBridge` state.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize,
)]
#[archive_attr(derive(CheckBytes))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PendingWithdrawal {
    /// Address of the sender on `DuskEVM`.
    pub from: EVMAddress,
    /// Address of the receiver on `DuskDS`.
    pub to: Address,
    /// Amount of DUSK sent.
    pub amount: u64,
    /// The block-height of the withdrawal request.
    pub block_height: u64,
}

// =========================================================================
// Helper functions
// =========================================================================

/// The raw size of a bls-key is the same as `bls12_381::G2Affine::RAW_SIZE`
const PK_RAW_SIZE: usize = 193;

/// Encodes a `DuskDS` public key into a byte vector suitable for inclusion in
/// `extra_data`.
#[must_use]
pub fn encode_ds_address(pk: PublicKey) -> Vec<u8> {
    let mut encoding = Vec::with_capacity(
        u64::SIZE + PublicKey::SIZE + u64::SIZE + PK_RAW_SIZE,
    );
    encoding.extend_from_slice(&(PublicKey::SIZE as u64).to_be_bytes()[..]);
    encoding.extend_from_slice(&pk.to_bytes()[..]);
    encoding.extend_from_slice(&(PK_RAW_SIZE as u64).to_be_bytes()[..]);
    encoding.extend_from_slice(&pk.to_raw_bytes()[..]);
    encoding
}

/// Decodes a `DuskDS` public key from the beginning of a byte slice.
///
/// # Errors
/// Returns an error if the encoded key sizes don't match expected values
/// or if the raw and compressed keys differ.
pub fn decode_ds_address(
    data: impl AsRef<[u8]>,
) -> Result<PublicKey, &'static str> {
    let data = data.as_ref();

    if data.len() < u64::SIZE + PublicKey::SIZE + u64::SIZE + PK_RAW_SIZE {
        return Err(error::INVALID_ENCODING);
    }

    let mut key_size_bytes = [0u8; u64::SIZE];
    key_size_bytes.copy_from_slice(&data[..u64::SIZE]);
    let key_size = u64::from_be_bytes(key_size_bytes);

    if key_size != PublicKey::SIZE as u64 {
        return Err(error::INVALID_ENCODING);
    }

    let mut raw_key_size_bytes = [0u8; u64::SIZE];
    let offset = u64::SIZE + PublicKey::SIZE;
    raw_key_size_bytes.copy_from_slice(&data[offset..offset + u64::SIZE]);
    let raw_key_size = u64::from_be_bytes(raw_key_size_bytes);

    if raw_key_size != PK_RAW_SIZE as u64 {
        return Err(error::INVALID_ENCODING);
    }

    let offset = 2 * u64::SIZE + PublicKey::SIZE;
    let pk = unsafe {
        PublicKey::from_slice_unchecked(&data[offset..offset + PK_RAW_SIZE])
    };

    if pk.to_bytes() != data[u64::SIZE..u64::SIZE + PublicKey::SIZE] {
        return Err(error::INVALID_ENCODING);
    }

    Ok(pk)
}

// =========================================================================
// OwnableUpgradeable trait
// =========================================================================

#[cfg(feature = "abi")]
/// Trait to implement the `OwnableUpgradeable` contract module functionality.
pub trait OwnableUpgradeable {
    /// Returns the address of the current owner.
    fn owner(&self) -> Option<Address>;

    /// Returns a mutable reference to the address of the current owner.
    fn owner_mut(&mut self) -> &mut Option<Address>;

    /// Transfers the authorized owner stored in the contract-state.
    fn transfer_ownership(&mut self, new_owner: Address) {
        use dusk_core::abi;
        self.only_owner();

        let previous_owner =
            core::mem::replace(self.owner_mut(), Some(new_owner))
                .expect(error::OWNABLE_INVALID_OWNER);

        abi::emit(
            events::OwnershipTransferred::OWNERSHIP_TRANSFERRED,
            events::OwnershipTransferred {
                previous_owner,
                new_owner: Some(new_owner),
            },
        );
    }

    /// Renounces the authorized owner stored in the contract-state.
    fn renounce_ownership(&mut self) {
        use dusk_core::abi;
        self.only_owner();

        let previous_owner = core::mem::take(self.owner_mut())
            .expect(error::OWNABLE_INVALID_OWNER);

        abi::emit(
            events::OwnershipTransferred::OWNERSHIP_RENOUNCED,
            events::OwnershipTransferred {
                previous_owner,
                new_owner: None,
            },
        );
    }

    /// Panics if called by any account other than the owner.
    fn only_owner(&self) {
        let tx_sender = initiator();
        let current_owner = self.owner().expect(error::OWNABLE_INVALID_OWNER);
        assert!(
            tx_sender == current_owner,
            "{}",
            error::OWNABLE_UNAUTHORIZED_ACCOUNT
        );
    }
}

/// Determines and returns the initiator of the current call.
#[cfg(feature = "abi")]
#[must_use]
fn initiator() -> Address {
    use dusk_core::abi;
    use dusk_core::transfer::TRANSFER_CONTRACT;

    match abi::callstack().len() {
        0 => {
            panic!(
                "determining the initiator of a contract query is meaningless"
            );
        }
        1 => {
            assert!(
                abi::caller()
                    .expect("since the callstack is 1, there is a caller")
                    == TRANSFER_CONTRACT
            );
            Address::External(
                abi::public_sender().expect(error::SHIELDED_NOT_SUPPORTED),
            )
        }
        _ => Address::Contract(
            abi::caller()
                .expect("since the callstack is > 1, there is a caller"),
        ),
    }
}

// =========================================================================
// Events module
// =========================================================================

/// Events emitted by the bridge contract.
pub mod events {
    use alloc::vec::Vec;

    use rkyv::{Archive, Deserialize, Serialize};

    use super::{Address, ContractId, EVMAddress};

    #[allow(unused_imports)]
    use rkyv::bytecheck::CheckBytes;

    /// Event emitted when the ownership of a contract is transferred.
    #[derive(
        Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize,
    )]
    #[archive_attr(derive(CheckBytes))]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct OwnershipTransferred {
        /// The previous owner of the contract.
        pub previous_owner: Address,
        /// The new owner of the contract, this will be `None` if ownership is
        /// renounced.
        pub new_owner: Option<Address>,
    }

    impl OwnershipTransferred {
        /// Event Topic for transferring the ownership.
        pub const OWNERSHIP_TRANSFERRED: &'static str = "ownership_transferred";
        /// Event Topic for renouncing the ownership.
        pub const OWNERSHIP_RENOUNCED: &'static str = "ownership_renounced";
    }

    /// This event records when the contract migrated to a new bridge.
    #[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
    #[archive_attr(derive(CheckBytes))]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct Migrated {
        /// The contract-ID of the new bridge contract.
        pub next_bridge: ContractId,
        /// The amount of bridged funds that are transferred to the new bridge.
        pub amount: u64,
    }

    impl Migrated {
        /// Event topic for migrating.
        pub const TOPIC: &'static str = "bridge_migrated";
    }

    /// This event records when the contract is paused or unpaused by the owner.
    #[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
    #[archive_attr(derive(CheckBytes))]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct PauseToggled();

    impl PauseToggled {
        /// Event topic for pausing the bridge.
        pub const PAUSED: &'static str = "bridge_paused";
        /// Event topic for unpausing the bridge.
        pub const UNPAUSED: &'static str = "bridge_unpaused";
    }

    /// Emitted when a `u64` state variable is updated by the contract owner.
    #[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
    #[archive_attr(derive(CheckBytes))]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct U64Set {
        /// The previous value.
        pub previous: u64,
        /// The new value.
        pub new: u64,
    }

    impl U64Set {
        /// Event topic for finalization period updates.
        pub const FINALIZATION_PERIOD: &'static str = "finalization_period_set";
        /// Event topic for deposit fee updates.
        pub const DEPOSIT_FEE: &'static str = "deposit_fee_set";
        /// Event topic for deposit gas limit updates.
        pub const DEPOSIT_GAS_LIMIT: &'static str = "deposit_gas_limit_set";
        /// Event topic for minimum gas limit updates.
        pub const MIN_GAS_LIMIT: &'static str = "min_gas_limit_set";
        /// Event topic for max data length updates.
        pub const MAX_DATA_LENGTH: &'static str = "max_data_length_set";
    }

    /// Emitted when an `EVMAddress` or `alias_offset` state variable is updated.
    #[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
    #[archive_attr(derive(CheckBytes))]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct EVMAddressOrOffsetSet {
        /// The previous `EVMAddress` or `alias_offset`.
        pub previous: EVMAddress,
        /// The new `EVMAddress` or `alias_offset`.
        pub new: EVMAddress,
    }

    impl EVMAddressOrOffsetSet {
        /// Event topic for updating the `EVMAddress` for `this_bridge_mapped`.
        pub const THIS_BRIDGE_MAPPED: &'static str = "this_bridge_mapped_set";
        /// Event topic for updating the `EVMAddress` for `this_messenger_mapped`.
        pub const THIS_MESSENGER_MAPPED: &'static str = "this_messenger_mapped_set";
        /// Event topic for updating the `EVMAddress` for `other_bridge`.
        pub const OTHER_BRIDGE: &'static str = "other_bridge_set";
        /// Event topic for updating the `EVMAddress` for `other_messenger`.
        pub const OTHER_MESSENGER: &'static str = "other_messenger_set";
        /// Event topic for updating the 20 bytes array for `alias_offset`.
        pub const ALIAS_OFFSET: &'static str = "alias_offset_set";
    }

    /// Emitted when a deposit is made to the bridge on this chain.
    #[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
    #[archive_attr(derive(CheckBytes))]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct TransactionDeposited {
        /// Address of the mapped sender contract on `DuskDS`.
        pub from: EVMAddress,
        /// Address of the receiver contract on `DuskEVM`.
        pub to: EVMAddress,
        /// The version of the deposit event.
        pub version: [u8; 32],
        /// The ABI encoded deposit data.
        pub opaque_data: Vec<u8>,
    }

    impl TransactionDeposited {
        /// Event topic for informing `DuskEVM` of the deposit transaction.
        pub const TOPIC: &'static str = "transaction_deposited";
    }

    /// Emitted when a bridge deposit is initiated.
    #[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
    #[archive_attr(derive(CheckBytes))]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct BridgeInitiated {
        /// Address of the sender on `DuskDS`, None if the deposit came from a
        /// shielded account.
        pub from: Option<Address>,
        /// Address of the receiver on `DuskEVM`.
        pub to: EVMAddress,
        /// Amount of DUSK sent in Lux.
        pub amount: u64,
        /// Fee for finishing the deposit on `DuskEVM` in Lux.
        pub deposit_fee: u64,
        /// Optional extra data sent with the transaction.
        pub extra_data: Vec<u8>,
    }

    impl BridgeInitiated {
        /// Event Topic for initiating the bridge on `DuskDS`.
        pub const TOPIC: &'static str = "bridge_initiated";
    }

    /// Emitted when a DUSK bridge withdrawal is finalized on this chain.
    #[derive(Debug, Clone, PartialEq, Eq, Archive, Serialize, Deserialize)]
    #[archive_attr(derive(CheckBytes))]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct BridgeFinalized {
        /// Address of the sender on `DuskEVM`.
        pub from: EVMAddress,
        /// Address of the receiver on `DuskDS`.
        pub to: Address,
        /// Amount of DUSK sent in Lux.
        pub amount: u64,
    }

    impl BridgeFinalized {
        /// Event Topic for finalizing the bridge on `DuskDS`.
        pub const TOPIC: &'static str = "bridge_finalized";
    }

    // Re-use PendingWithdrawal as an event type
    pub use super::PendingWithdrawal;

    impl PendingWithdrawal {
        /// Event Topic for adding a withdrawal request on `DuskDS`.
        pub const ADDED: &'static str = "withdrawal_added";
        /// Event Topic for removing a withdrawal request on `DuskDS` without it
        /// being finalized.
        pub const REMOVED: &'static str = "withdrawal_removed";
    }
}

// =========================================================================
// Error constants
// =========================================================================

/// Error constants.
pub mod error {
    /// Error thrown when the caller is a shielded address.
    pub const SHIELDED_NOT_SUPPORTED: &str = "The owner cannot be shielded.";

    /// Error thrown when a caller account, different from the owner, is trying
    /// to perform an operation only the owner is authorized to execute.
    pub const OWNABLE_UNAUTHORIZED_ACCOUNT: &str =
        "The caller account is not authorized to perform this operation.";

    /// Error thrown when the owner should be set but isn't.
    pub const OWNABLE_INVALID_OWNER: &str =
        "The owner is not a valid owner account, e.g. `None`.";

    /// Error message given when the `to` `DuskDS` key in not encoded correctly
    /// in the `extra_data` field of the withdrawal request.
    pub const INVALID_ENCODING: &str = "The `DuskDS` encoding is not valid.";
}

// =========================================================================
// Serde support for EVMAddress
// =========================================================================

#[cfg(feature = "serde")]
mod serde_evm {
    use alloc::format;

    pub(super) fn serialize<S>(
        addr: &[u8; 20],
        ser: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let hex: alloc::string::String =
            addr.iter().fold(alloc::string::String::from("0x"), |mut s, b| {
                use core::fmt::Write;
                let _ = write!(s, "{b:02x}");
                s
            });
        ser.serialize_str(&hex)
    }

    pub(super) fn deserialize<'de, D>(de: D) -> Result<[u8; 20], D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = [u8; 20];

            fn expecting(
                &self,
                f: &mut alloc::fmt::Formatter,
            ) -> alloc::fmt::Result {
                f.write_str(
                    r#"a hex string for 20 bytes, with or without "0x" prefix"#,
                )
            }

            fn visit_str<E>(self, mut s: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                s = s.trim();
                if let Some(rest) =
                    s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))
                {
                    s = rest;
                }

                if s.len() != 40 {
                    return Err(E::custom(format!(
                        "expected 40 hex chars, got {}",
                        s.len()
                    )));
                }

                let mut addr = [0u8; 20];
                for (i, byte) in addr.iter_mut().enumerate() {
                    *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
                        .map_err(|_| {
                            E::custom("invalid hex string for 20 byte address")
                        })?;
                }
                Ok(addr)
            }
        }

        de.deserialize_str(V)
    }
}
