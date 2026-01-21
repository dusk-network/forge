/// Proof-of-Concept: Single Source of Truth Design
///
/// Everything is derived from the `#[contract]` annotation on state.rs.
/// This file shows what developers write vs what gets generated.

// ============================================================================
// WHAT THE DEVELOPER WRITES (state.rs)
// ============================================================================

// This is the ONLY place where function signatures are defined.
// Everything else is generated from this.

#[contract]
impl StandardBridge {
    /// Initializes the contract with an owner.
    pub fn init(&mut self, owner: DSAddress) {
        assert!(!self.initialized);
        self.owner = Some(owner);
        self.initialized = true;
    }

    /// Returns whether the bridge is paused.
    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    /// Pauses the bridge. Only callable by owner.
    pub fn pause(&mut self) {
        self.only_owner();
        self.is_paused = true;
        abi::emit("paused", events::PauseToggled());
    }

    /// Unpauses the bridge.
    pub fn unpause(&mut self) {
        self.only_owner();
        self.is_paused = false;
        abi::emit("unpaused", events::PauseToggled());
    }

    /// Returns the finalization period in blocks.
    pub fn finalization_period(&self) -> u64 {
        self.finalization_period
    }

    /// Returns the deposit fee.
    pub fn deposit_fee(&self) -> u64 {
        self.deposit_fee
    }

    /// Updates a u64 configuration value.
    pub fn set_u64(&mut self, new_value: SetU64) {
        self.only_owner();
        let topic = match &new_value {
            SetU64::FinalizationPeriod(_) => "finalization_period",
            SetU64::DepositFee(_) => "deposit_fee",
            // ...
        };
        // ... implementation
        abi::emit(topic, events::U64Set { previous, new });
    }

    /// Returns the other bridge address.
    pub fn other_bridge(&self) -> EVMAddress {
        self.other_bridge
    }

    /// Updates an EVM address configuration.
    pub fn set_evm_address_or_offset(&mut self, new_value: SetEVMAddressOrOffset) {
        self.only_owner();
        // ... implementation
        abi::emit("address_set", events::EVMAddressOrOffsetSet { ... });
    }

    /// Deposits funds to L2.
    pub fn deposit(&mut self, deposit: Deposit) {
        self.assert_not_paused();
        // ... validation and logic
        abi::emit("transaction_deposited", events::TransactionDeposited { ... });
        abi::emit("bridge_initiated", events::BridgeInitiated { ... });
    }

    /// Returns a pending withdrawal by ID.
    pub fn pending_withdrawal(&self, id: WithdrawalId) -> Option<PendingWithdrawal> {
        self.pending_withdrawals.get(&id).cloned()
    }

    /// Adds a pending withdrawal (called by relayer).
    pub fn add_pending_withdrawal(&mut self, request: WithdrawalRequest) {
        // ... implementation
        abi::emit("pending_withdrawal_added", events::PendingWithdrawal { ... });
    }

    /// Finalizes a withdrawal after the finalization period.
    pub fn finalize_withdrawal(&mut self, id: WithdrawalId) {
        // ... implementation
        abi::emit("bridge_finalized", events::BridgeFinalized { ... });
    }

    /// Checks if a withdrawal is finalized.
    pub fn is_finalized(&self, id: WithdrawalId) -> bool {
        self.finalized_withdrawals.contains(&id)
    }

    /// Returns the contract owner.
    pub fn owner(&self) -> Option<DSAddress> {
        self.owner.clone()
    }

    /// Transfers ownership to a new address.
    pub fn transfer_ownership(&mut self, new_owner: DSAddress) {
        self.only_owner();
        let previous = self.owner.replace(new_owner.clone());
        abi::emit("ownership_transferred", events::OwnershipTransferred { previous, new: new_owner });
    }

    /// Returns the contract version.
    pub fn version(&self) -> String {
        "1.0.0".to_owned()
    }

    /// Custom serialization - needs manual handler.
    #[contract(custom)]
    pub fn extra_data(&self, pk: PublicKey) -> Vec<u8> {
        encode_ds_address(pk)
    }

    // Private helper - NOT exported (no `pub`)
    fn only_owner(&self) {
        assert!(self.owner.is_some(), "No owner set");
        // ...
    }

    // Private helper - NOT exported
    fn assert_not_paused(&self) {
        assert!(!self.is_paused, "Contract is paused");
    }
}

// ============================================================================
// WHAT THE MACRO GENERATES (in same crate)
// ============================================================================

// 1. CONTRACT_SCHEMA - exported for use by data-driver
pub const CONTRACT_SCHEMA: Schema = Schema {
    name: "StandardBridge",
    functions: &[
        // Extracted from pub fn signatures
        Function { name: "init", input: type_of::<DSAddress>(), output: type_of::<()>(), custom: false,
                   doc: "Initializes the contract with an owner." },
        Function { name: "is_paused", input: type_of::<()>(), output: type_of::<bool>(), custom: false,
                   doc: "Returns whether the bridge is paused." },
        Function { name: "pause", input: type_of::<()>(), output: type_of::<()>(), custom: false,
                   doc: "Pauses the bridge. Only callable by owner." },
        Function { name: "unpause", input: type_of::<()>(), output: type_of::<()>(), custom: false,
                   doc: "Unpauses the bridge." },
        Function { name: "finalization_period", input: type_of::<()>(), output: type_of::<u64>(), custom: false,
                   doc: "Returns the finalization period in blocks." },
        Function { name: "deposit_fee", input: type_of::<()>(), output: type_of::<u64>(), custom: false,
                   doc: "Returns the deposit fee." },
        Function { name: "set_u64", input: type_of::<SetU64>(), output: type_of::<()>(), custom: false,
                   doc: "Updates a u64 configuration value." },
        Function { name: "other_bridge", input: type_of::<()>(), output: type_of::<EVMAddress>(), custom: false,
                   doc: "Returns the other bridge address." },
        Function { name: "set_evm_address_or_offset", input: type_of::<SetEVMAddressOrOffset>(), output: type_of::<()>(), custom: false,
                   doc: "Updates an EVM address configuration." },
        Function { name: "deposit", input: type_of::<Deposit>(), output: type_of::<()>(), custom: false,
                   doc: "Deposits funds to L2." },
        Function { name: "pending_withdrawal", input: type_of::<WithdrawalId>(), output: type_of::<Option<PendingWithdrawal>>(), custom: false,
                   doc: "Returns a pending withdrawal by ID." },
        Function { name: "add_pending_withdrawal", input: type_of::<WithdrawalRequest>(), output: type_of::<()>(), custom: false,
                   doc: "Adds a pending withdrawal (called by relayer)." },
        Function { name: "finalize_withdrawal", input: type_of::<WithdrawalId>(), output: type_of::<()>(), custom: false,
                   doc: "Finalizes a withdrawal after the finalization period." },
        Function { name: "is_finalized", input: type_of::<WithdrawalId>(), output: type_of::<bool>(), custom: false,
                   doc: "Checks if a withdrawal is finalized." },
        Function { name: "owner", input: type_of::<()>(), output: type_of::<Option<DSAddress>>(), custom: false,
                   doc: "Returns the contract owner." },
        Function { name: "transfer_ownership", input: type_of::<DSAddress>(), output: type_of::<()>(), custom: false,
                   doc: "Transfers ownership to a new address." },
        Function { name: "version", input: type_of::<()>(), output: type_of::<String>(), custom: false,
                   doc: "Returns the contract version." },
        Function { name: "extra_data", input: type_of::<PublicKey>(), output: type_of::<Vec<u8>>(), custom: true,
                   doc: "Custom serialization - needs manual handler." },
    ],
    events: &[
        // Extracted from abi::emit() calls
        Event { topic: "paused", data: type_of::<events::PauseToggled>() },
        Event { topic: "unpaused", data: type_of::<events::PauseToggled>() },
        Event { topic: "finalization_period", data: type_of::<events::U64Set>() },
        Event { topic: "deposit_fee", data: type_of::<events::U64Set>() },
        Event { topic: "address_set", data: type_of::<events::EVMAddressOrOffsetSet>() },
        Event { topic: "transaction_deposited", data: type_of::<events::TransactionDeposited>() },
        Event { topic: "bridge_initiated", data: type_of::<events::BridgeInitiated>() },
        Event { topic: "pending_withdrawal_added", data: type_of::<events::PendingWithdrawal>() },
        Event { topic: "bridge_finalized", data: type_of::<events::BridgeFinalized>() },
        Event { topic: "ownership_transferred", data: type_of::<events::OwnershipTransferred>() },
    ],
};

// 2. EXTERN "C" WRAPPERS - replaces manual lib.rs
#[no_mangle]
unsafe extern "C" fn init(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |owner: DSAddress| STATE.init(owner))
}

#[no_mangle]
unsafe extern "C" fn is_paused(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |(): ()| STATE.is_paused())
}

#[no_mangle]
unsafe extern "C" fn pause(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |(): ()| STATE.pause())
}

#[no_mangle]
unsafe extern "C" fn unpause(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |(): ()| STATE.unpause())
}

#[no_mangle]
unsafe extern "C" fn finalization_period(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |(): ()| STATE.finalization_period())
}

#[no_mangle]
unsafe extern "C" fn deposit_fee(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |(): ()| STATE.deposit_fee())
}

#[no_mangle]
unsafe extern "C" fn set_u64(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |v: SetU64| STATE.set_u64(v))
}

#[no_mangle]
unsafe extern "C" fn other_bridge(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |(): ()| STATE.other_bridge())
}

#[no_mangle]
unsafe extern "C" fn set_evm_address_or_offset(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |v: SetEVMAddressOrOffset| STATE.set_evm_address_or_offset(v))
}

#[no_mangle]
unsafe extern "C" fn deposit(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |d: Deposit| STATE.deposit(d))
}

#[no_mangle]
unsafe extern "C" fn pending_withdrawal(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |id: WithdrawalId| STATE.pending_withdrawal(id))
}

#[no_mangle]
unsafe extern "C" fn add_pending_withdrawal(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |r: WithdrawalRequest| STATE.add_pending_withdrawal(r))
}

#[no_mangle]
unsafe extern "C" fn finalize_withdrawal(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |id: WithdrawalId| STATE.finalize_withdrawal(id))
}

#[no_mangle]
unsafe extern "C" fn is_finalized(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |id: WithdrawalId| STATE.is_finalized(id))
}

#[no_mangle]
unsafe extern "C" fn owner(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |(): ()| STATE.owner())
}

#[no_mangle]
unsafe extern "C" fn transfer_ownership(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |new: DSAddress| STATE.transfer_ownership(new))
}

#[no_mangle]
unsafe extern "C" fn version(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |(): ()| STATE.version())
}

#[no_mangle]
unsafe extern "C" fn extra_data(arg_len: u32) -> u32 {
    abi::wrap_call(arg_len, |pk: PublicKey| STATE.extra_data(pk))
}

// ============================================================================
// DATA-DRIVER CRATE (entire file!)
// ============================================================================

// data-drivers/StandardBridge/src/lib.rs

use standard_bridge::CONTRACT_SCHEMA;

// This single macro generates the entire ConvertibleContract implementation
generate_data_driver!(CONTRACT_SCHEMA);

// Only custom handlers need to be written manually
#[custom_handler(extra_data, encode)]
fn encode_extra_data(json: &str) -> Result<Vec<u8>, Error> {
    let pk: PublicKey = serde_json::from_str(json)?;
    Ok(encode_ds_address(pk))
}

#[custom_handler(extra_data, decode)]
fn decode_extra_data(rkyv: &[u8]) -> Result<JsonValue, Error> {
    let pk = decode_ds_address(rkyv)?;
    Ok(serde_json::to_value(pk)?)
}

// ============================================================================
// WHAT generate_data_driver! EXPANDS TO
// ============================================================================

pub struct ContractDriver;

impl Default for ContractDriver {
    fn default() -> Self { Self }
}

impl ConvertibleContract for ContractDriver {
    fn encode_input_fn(&self, fn_name: &str, json: &str) -> Result<Vec<u8>, Error> {
        match fn_name {
            "init" => json_to_rkyv::<DSAddress>(json),
            "is_paused" | "pause" | "unpause" | "finalization_period" | "deposit_fee"
            | "other_bridge" | "owner" | "version" => json_to_rkyv::<()>(json),
            "set_u64" => json_to_rkyv::<SetU64>(json),
            "set_evm_address_or_offset" => json_to_rkyv::<SetEVMAddressOrOffset>(json),
            "deposit" => json_to_rkyv::<Deposit>(json),
            "pending_withdrawal" | "finalize_withdrawal" | "is_finalized" => json_to_rkyv::<WithdrawalId>(json),
            "add_pending_withdrawal" => json_to_rkyv::<WithdrawalRequest>(json),
            "transfer_ownership" => json_to_rkyv::<DSAddress>(json),
            "extra_data" => encode_extra_data(json),  // Custom handler
            name => Err(Error::Unsupported(format!("fn {name}"))),
        }
    }

    fn decode_input_fn(&self, fn_name: &str, rkyv: &[u8]) -> Result<JsonValue, Error> {
        match fn_name {
            "init" | "transfer_ownership" => rkyv_to_json::<DSAddress>(rkyv),
            "set_u64" => rkyv_to_json::<SetU64>(rkyv),
            "set_evm_address_or_offset" => rkyv_to_json::<SetEVMAddressOrOffset>(rkyv),
            "deposit" => rkyv_to_json::<Deposit>(rkyv),
            "pending_withdrawal" | "finalize_withdrawal" | "is_finalized" => rkyv_to_json::<WithdrawalId>(rkyv),
            "add_pending_withdrawal" => rkyv_to_json::<WithdrawalRequest>(rkyv),
            _ => rkyv_to_json::<()>(rkyv),
        }
    }

    fn decode_output_fn(&self, fn_name: &str, rkyv: &[u8]) -> Result<JsonValue, Error> {
        match fn_name {
            "is_paused" | "is_finalized" => rkyv_to_json::<bool>(rkyv),
            "finalization_period" | "deposit_fee" => rkyv_to_json_u64(rkyv),
            "other_bridge" => rkyv_to_json::<EVMAddress>(rkyv),
            "pending_withdrawal" => rkyv_to_json::<Option<PendingWithdrawal>>(rkyv),
            "owner" => rkyv_to_json::<Option<DSAddress>>(rkyv),
            "version" => rkyv_to_json::<String>(rkyv),
            "extra_data" => decode_extra_data(rkyv),  // Custom handler
            _ => Ok(JsonValue::Null),  // Functions returning ()
        }
    }

    fn decode_event(&self, event_name: &str, rkyv: &[u8]) -> Result<JsonValue, Error> {
        match event_name {
            "paused" | "unpaused" => rkyv_to_json::<events::PauseToggled>(rkyv),
            "finalization_period" | "deposit_fee" => rkyv_to_json::<events::U64Set>(rkyv),
            "address_set" => rkyv_to_json::<events::EVMAddressOrOffsetSet>(rkyv),
            "transaction_deposited" => rkyv_to_json::<events::TransactionDeposited>(rkyv),
            "bridge_initiated" => rkyv_to_json::<events::BridgeInitiated>(rkyv),
            "pending_withdrawal_added" => rkyv_to_json::<events::PendingWithdrawal>(rkyv),
            "bridge_finalized" => rkyv_to_json::<events::BridgeFinalized>(rkyv),
            "ownership_transferred" => rkyv_to_json::<events::OwnershipTransferred>(rkyv),
            event => Err(Error::Unsupported(format!("event {event}"))),
        }
    }

    fn get_schema(&self) -> String {
        serde_json::to_string_pretty(&CONTRACT_SCHEMA.to_json()).unwrap()
    }
}

// ============================================================================
// SUMMARY
// ============================================================================

/*
SINGLE SOURCE OF TRUTH: state.rs

The #[contract] macro on the impl block:
1. Parses all `pub fn` methods → extracts function signatures
2. Parses all `abi::emit()` calls → extracts event definitions
3. Generates CONTRACT_SCHEMA constant
4. Generates extern "C" wrappers

The generate_data_driver! macro:
1. Reads CONTRACT_SCHEMA
2. Generates ConvertibleContract implementation
3. Hooks up custom handlers

WHAT DEVELOPERS WRITE:
- state.rs: Business logic with #[contract] annotation (~unchanged)
- data-driver: ~15 lines (just custom handlers)

WHAT'S AUTO-GENERATED:
- Extern "C" wrappers (currently 34 hand-written functions)
- Contract schema
- Data-driver implementation (currently 300 lines)
- JSON schema for clients

ADDING A NEW FUNCTION:
1. Add `pub fn new_function(...)` to state.rs
2. Done. Everything else updates automatically.
*/
