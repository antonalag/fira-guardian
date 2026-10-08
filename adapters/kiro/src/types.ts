// §18 request/response shapes and a read-only model of the canonical
// `report.json` (S11) produced by the Rust auditor.
//
// These are hand-written TypeScript interfaces mirroring the frozen wire shapes;
// they are NOT generated from Rust and import nothing from the Rust workspace.
// The adapter uses them only to *project* the canonical report read-only — it
// never recomputes, reinterprets, adjudicates, or modifies any field (§9.2).

/** The five built-in profiles (PROF-1). An unknown value is never accepted. */
export type ProfileId =
  | "library"
  | "cli_tool"
  | "web_service"
  | "stateful_distributed"
  | "batch_pipeline";

export const PROFILE_IDS: readonly ProfileId[] = [
  "library",
  "cli_tool",
  "web_service",
  "stateful_distributed",
  "batch_pipeline",
] as const;

/** Requested audit depth (S1). CLI accepts the lowercase forms. */
export type Depth = "shallow" | "standard" | "deep";

export const DEPTHS: readonly Depth[] = ["shallow", "standard", "deep"] as const;

/**
 * A Kiro-side request in the §18 `AuditRequest` shape (MVP subset). Mirrors the
 * existing fields; adds no new field. `execution_boundary` is intentionally
 * absent — it is fixed to {READ, EXECUTE_EXISTING} by the CLI and must not be
 * settable here (CAP-1).
 */
export interface KiroAuditRequest {
  readonly projectRoot: string;
  readonly releaseTarget: string;
  readonly auditProfile?: ProfileId;
  readonly requestedDepth?: Depth;
  /** WS-1 audit-workspace base; never the project tree. */
  readonly workspace?: string;
}

/** How to find and trust the auditor binary (§9.5). */
export interface AuditorBinary {
  /** Explicit configured path; when absent, resolve `fira-guardian` on `$PATH`. */
  readonly path?: string;
  /**
   * The contract version the caller expects the binary to target. The adapter
   * asserts `fira-guardian version` reports exactly this; mismatch fails closed.
   */
  readonly expectedContractVersion: string;
}

/** The human's confirm-gate decision (§12 P1 / §10). PROF-1: reclassify only to
 * one of the five. The adapter never fabricates an ACCEPTED/REJECTED decision. */
export type ConfirmDecision =
  | { readonly kind: "confirmProposed" }
  | { readonly kind: "reclassify"; readonly profile: ProfileId };

// ---------------------------------------------------------------------------
// Read-only projection of the canonical report.json (S11). Field names match
// the frozen wire form (snake_case). Only the fields the §18 AuditResponse needs
// are modeled; unknown fields are ignored by the read-only projection.
// ---------------------------------------------------------------------------

export type TechnicalAssessmentResult = "READY" | "READY_WITH_RISKS" | "NOT_READY";
export type HumanDecisionFiraState = "NOT_APPLICABLE" | "PENDING";
export type GateState = "PASS" | "FAIL" | "PARTIAL" | "UNKNOWN" | "N/A";
export type LifecycleStatus =
  | "OPEN"
  | "FIX_CLAIMED"
  | "VERIFIED_FIXED"
  | "REOPENED"
  | "WONT_FIX";
export type Severity = "CRITICAL" | "HIGH" | "MEDIUM" | "LOW" | "INFO";

/** S10 TechnicalAssessment (as serialized in report.json). */
export interface TechnicalAssessment {
  readonly result: TechnicalAssessmentResult;
  readonly blocking_findings: readonly string[];
  readonly blocking_gates: readonly string[];
  readonly risk_findings: readonly string[];
  readonly risk_gates: readonly string[];
  readonly computed_by_rule: string;
}

/** S10 FIRA-produced human decision (restricted to {NOT_APPLICABLE, PENDING}). */
export interface HumanDecisionFira {
  readonly state: HumanDecisionFiraState;
}

/** S7 Finding (projection subset). */
export interface Finding {
  readonly id: string;
  readonly severity: Severity;
  readonly lifecycle_status: LifecycleStatus;
  readonly regression: boolean;
  readonly title: string;
  // Other S7 fields are present in the JSON and preserved in `machineReport`;
  // this projection surfaces the ones a host typically renders. The canonical
  // bytes remain authoritative.
}

/** S8 Gate (projection subset). */
export interface Gate {
  readonly name: string;
  readonly state: GateState;
  readonly requirement_level: "required" | "recommended" | "not_applicable";
}

/** S9 CoverageStatement (projection; opaque sub-structures kept as `unknown`). */
export interface CoverageStatement {
  readonly audited_areas: readonly unknown[];
  readonly skipped_areas: readonly unknown[];
  readonly blocked_areas: readonly unknown[];
  readonly executed_mechanisms: readonly unknown[];
  readonly known_limitations: readonly string[];
  readonly assumptions: readonly string[];
  readonly unanchored_hypotheses: readonly string[];
}

/** The subset of S11 report.json this adapter reads. Unknown fields ignored. */
export interface CanonicalReport {
  readonly schema_version: string;
  readonly audit_id: string;
  readonly technical_assessment: TechnicalAssessment;
  readonly human_decision: HumanDecisionFira;
  readonly findings: readonly Finding[];
  readonly gates: readonly Gate[];
  readonly coverage: CoverageStatement;
}

/**
 * The §18 `AuditResponse`, built from the CLI output + WS-1 artifacts. The
 * structured fields are a read-only projection of the canonical `report.json`;
 * `machineReport`/`humanReport` are the canonical bytes FIRA produced and are
 * the source of truth (§9.2).
 */
export interface KiroAuditResponse {
  readonly technicalAssessment: TechnicalAssessment;
  readonly humanDecision: HumanDecisionFira;
  readonly findings: readonly Finding[];
  readonly gates: readonly Gate[];
  readonly coverage: CoverageStatement;
  /** Canonical rendered JSON FIRA produced (`report.json` bytes). Authoritative. */
  readonly machineReport: string;
  /** Canonical rendered Markdown FIRA produced (`report.md` bytes). */
  readonly humanReport: string;
  /** The `<base>/<audit-id>/` directory (WS-1). */
  readonly canonicalLocation: string;
}

/** The confirm-gate proposal surfaced in phase 1 (projection of the CLI's P1
 * classify/confirm output). The adapter invents no gate semantics (PROF-1). */
export interface ConfirmGateProposal {
  /** The profile the classifier proposed, when the project was classifiable. */
  readonly proposedProfile?: ProfileId;
  /** True when classification was Undetermined and a human must pick a profile. */
  readonly undetermined: boolean;
  /** The CLI's human-readable gate message (verbatim), for the host to display. */
  readonly message: string;
}
