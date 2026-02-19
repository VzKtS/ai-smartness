//! Injector -- format injection payload using the provider's formatter.

use crate::provider::{AiProvider, InjectionPayload};

pub struct Injector;

impl Injector {
    /// Format injection payload using the provider's format.
    pub fn inject(provider: &dyn AiProvider, payload: &InjectionPayload) -> String {
        provider.format_injection(payload)
    }
}
