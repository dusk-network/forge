// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// Copyright (c) DUSK NETWORK. All rights reserved.

//! Schema types for contract metadata.
//!
//! These types are used by the `#[contract]` macro to generate
//! compile-time contract schemas that describe functions and events.

use serde::Serialize;

/// Schema for a contract function.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct FunctionSchema {
    /// Function name.
    pub name: &'static str,
    /// Documentation string.
    pub doc: &'static str,
    /// Input type name (or "()" for no input).
    pub input: &'static str,
    /// Output type name (or "()" for no output).
    pub output: &'static str,
    /// Whether this function requires custom serialization.
    pub custom: bool,
}

/// Schema for a contract event.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct EventSchema {
    /// Event topic string.
    pub topic: &'static str,
    /// Event data type name.
    pub data: &'static str,
}

/// Complete schema for a contract.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ContractSchema {
    /// Contract name.
    pub name: &'static str,
    /// List of contract functions.
    pub functions: &'static [FunctionSchema],
    /// List of contract events.
    pub events: &'static [EventSchema],
}

impl ContractSchema {
    /// Returns an iterator over all functions.
    pub fn iter_functions(&self) -> impl Iterator<Item = &FunctionSchema> {
        self.functions.iter()
    }

    /// Returns an iterator over all events.
    pub fn iter_events(&self) -> impl Iterator<Item = &EventSchema> {
        self.events.iter()
    }

    /// Find a function by name.
    #[must_use]
    pub fn get_function(&self, name: &str) -> Option<&FunctionSchema> {
        self.functions.iter().find(|f| f.name == name)
    }

    /// Find an event by topic.
    #[must_use]
    pub fn get_event(&self, topic: &str) -> Option<&EventSchema> {
        self.events.iter().find(|e| e.topic == topic)
    }
}
