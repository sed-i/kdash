//! PVC-specific troubleshooting checks.
//!
//! This module inspects cached PVC state and produces [`DisplayFinding`]s for
//! PVCs that are in an unhealthy or noteworthy phase.
//!
//! References:
//! - <https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.35/#persistentvolumeclaimstatus-v1-core>

use super::{
  models::KubeResource,
  pvcs::KubePVC,
  troubleshoot::{DisplayFinding, Finding, IntoDisplayFinding, ResourceKind},
};

// ---------------------------------------------------------------------------
// PvcFinding — resource-specific finding data for PVCs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct PvcFinding {
  pub id: String,
  pub reason: String,
  pub namespace: String,
  pub pvc_name: String,
  pub message: String,
  pub age: String,
}

// ---------------------------------------------------------------------------
// Finding<PvcFinding> → DisplayFinding conversion
// ---------------------------------------------------------------------------

impl IntoDisplayFinding for Finding<PvcFinding> {
  fn into_display_finding(self) -> DisplayFinding {
    let severity = self.severity_tag();
    let inner = self.into_inner();
    DisplayFinding {
      severity,
      reason: inner.reason,
      resource_kind: ResourceKind::Pvc,
      namespace: Some(inner.namespace.clone()),
      resource_name: inner.pvc_name.clone(),
      message: inner.message,
      age: inner.age,
      describe_kind: "persistentvolumeclaim".into(),
      describe_name: inner.pvc_name,
      describe_namespace: Some(inner.namespace),
      k8s_obj: (),
    }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract `pvc.status.phase`, falling back to `"Unknown"`.
fn pvc_phase(pvc: &KubePVC) -> &str {
  pvc
    .get_k8s_obj()
    .status
    .as_ref()
    .and_then(|s| s.phase.as_deref())
    .unwrap_or("Unknown")
}

// ---------------------------------------------------------------------------
// Check type alias
// ---------------------------------------------------------------------------

/// A PVC check is a function that inspects a single PVC and optionally
/// produces a finding.
pub type PvcCheck = fn(&KubePVC) -> Option<Finding<PvcFinding>>;

// ---------------------------------------------------------------------------
// Individual PVC checks
// ---------------------------------------------------------------------------

/// Detect PVCs whose phase is not `Bound`.
fn check_pvc_phase(pvc: &KubePVC) -> Option<Finding<PvcFinding>> {
  let phase = pvc_phase(pvc);

  if phase == "Bound" {
    return None;
  }

  Some(Finding::Warn(PvcFinding {
    id: "pvc.phase.not_bound".into(),
    reason: phase.into(),
    namespace: pvc.namespace.clone(),
    pvc_name: pvc.name.clone(),
    message: format!("PVC phase is {}", phase),
    age: pvc.age.clone(),
  }))
}

// ---------------------------------------------------------------------------
// Registry of all PVC checks
// ---------------------------------------------------------------------------

/// Returns all registered PVC checks. Add new checks here.
fn all_pvc_checks() -> Vec<PvcCheck> {
  vec![check_pvc_phase]
}

// ---------------------------------------------------------------------------
// PVC evaluation entry point
// ---------------------------------------------------------------------------

/// Run every registered PVC check against every PVC and return the flattened
/// display findings.
pub fn evaluate_pvc_findings(pvcs: &[KubePVC]) -> Vec<DisplayFinding> {
  let checks = all_pvc_checks();

  pvcs
    .iter()
    .flat_map(|pvc| {
      checks
        .iter()
        .filter_map(move |check| check(pvc).map(|f| f.into_display_finding()))
    })
    .collect()
}
