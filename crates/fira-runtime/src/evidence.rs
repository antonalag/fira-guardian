//! Evidence collection (§17): capture an execution result as an `EvidenceRef`
//! and attach support mappings. No adjudication; attaching only appends.

use fira_core::evidence::{EvidenceRef, SupportMapping};
use fira_core::execution::ExecutionResult;
use fira_core::finding::Finding;
use fira_core::gate::Gate;
use fira_core::interfaces::{CapabilityError, EvidenceCollector};
use fira_core::model::EvidenceType;

/// The RUNTIME evidence collector.
#[derive(Debug, Clone, Default)]
pub struct EvidenceCollectorImpl;

impl EvidenceCollector for EvidenceCollectorImpl {
    fn capture(&self, execution_result: &ExecutionResult) -> Result<EvidenceRef, CapabilityError> {
        // A COMMAND evidence ref whose captured_output_ref is the command_id
        // (satisfies VR3: captured_output_ref present iff type=COMMAND).
        Ok(EvidenceRef {
            ref_id: execution_result.command_id.clone(),
            evidence_type: EvidenceType::Command,
            locator: execution_result.command_id.0.clone(),
            valid: true,
            captured_output_ref: Some(execution_result.command_id.clone()),
        })
    }

    fn attach_to_finding(
        &self,
        finding: &mut Finding,
        mapping: SupportMapping,
    ) -> Result<(), CapabilityError> {
        finding.support_mappings.push(mapping);
        Ok(())
    }

    fn attach_to_gate(
        &self,
        gate: &mut Gate,
        mapping: SupportMapping,
    ) -> Result<(), CapabilityError> {
        gate.support_mappings.push(mapping);
        Ok(())
    }
}
