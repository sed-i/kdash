//! Pod-specific troubleshooting checks.
//!
//! This module inspects cached pod state and produces [`DisplayFinding`]s for
//! pods that are in an unhealthy or noteworthy phase.
//!
//! The checks are based on the Kubernetes pod lifecycle model.  Pod phase is a
//! high-level summary of where a pod is in its lifecycle.
//!
//! References:
//! - <https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.35/#podstatus-v1-core>

use k8s_openapi::api::core::v1::PodCondition;

use super::{
  models::KubeResource,
  pods::KubePod,
  troubleshoot::{DisplayFinding, Finding, IntoDisplayFinding, ResourceKind},
};

// ---------------------------------------------------------------------------
// PodFinding — resource-specific finding data for pods
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct PodFinding {
  pub id: String,
  pub reason: String,
  pub namespace: String,
  pub pod_name: String,
  pub message: String,
  pub age: String,
}

// ---------------------------------------------------------------------------
// Finding<PodFinding> → DisplayFinding conversion
// ---------------------------------------------------------------------------

impl IntoDisplayFinding for Finding<PodFinding> {
  fn into_display_finding(self) -> DisplayFinding {
    let severity = self.severity_tag();
    let inner = self.into_inner();
    DisplayFinding {
      severity,
      reason: inner.reason,
      resource_kind: ResourceKind::Pod,
      namespace: Some(inner.namespace.clone()),
      resource_name: inner.pod_name.clone(),
      message: inner.message,
      age: inner.age,
      describe_kind: "pod".into(),
      describe_name: inner.pod_name,
      describe_namespace: Some(inner.namespace),
      k8s_obj: (),
    }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract `pod.status.phase`, falling back to `"Unknown"`.
fn pod_phase(pod: &KubePod) -> &str {
  pod
    .get_k8s_obj()
    .status
    .as_ref()
    .and_then(|s| s.phase.as_deref())
    .unwrap_or("Unknown")
}

/// Return the most recent pod condition, sorted by `lastTransitionTime` descending.
fn latest_condition(pod: &KubePod) -> Option<&PodCondition> {
  let mut conditions: Vec<&PodCondition> = pod
    .get_k8s_obj()
    .status
    .as_ref()
    .and_then(|s| s.conditions.as_ref())
    .map(|c| c.iter().collect())
    .unwrap_or_default();

  conditions.sort_by(|a, b| b.last_transition_time.cmp(&a.last_transition_time));

  conditions.into_iter().next()
}

/// Extract `.reason` from the most recent pod condition, falling back to `"N/A"`.
fn pod_status_reason(pod: &KubePod) -> String {
  latest_condition(pod)
    .and_then(|c| c.reason.as_deref())
    .unwrap_or("N/A")
    .into()
}

/// Extract `.message` from the most recent pod condition, falling back to `"N/A"`.
fn pod_status_message(pod: &KubePod) -> String {
  latest_condition(pod)
    .and_then(|c| c.message.as_deref())
    .unwrap_or("N/A")
    .into()
}

// ---------------------------------------------------------------------------
// Check type alias
// ---------------------------------------------------------------------------

/// A pod check is a function that inspects a single pod and optionally
/// produces a finding.
pub type PodCheck = fn(&KubePod) -> Option<Finding<PodFinding>>;

// ---------------------------------------------------------------------------
// Individual pod checks
// ---------------------------------------------------------------------------

/// Detect pods in an unhealthy phase (`Failed`, `Unknown`, or `Pending`).
/// References:
/// - <https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#pod-phase>
fn check_pod_phase(pod: &KubePod) -> Option<Finding<PodFinding>> {
  let phase = pod_phase(pod);

  let (id, finding_ctor): (&str, fn(PodFinding) -> Finding<PodFinding>) = match phase {
    "Failed" => ("pod.phase.failed", Finding::Error),
    "Unknown" => ("pod.phase.unknown", Finding::Warn),
    "Pending" => ("pod.phase.pending", Finding::Info),
    _ => return None,
  };

  Some(finding_ctor(PodFinding {
    id: id.into(),
    reason: pod_status_reason(pod),
    namespace: pod.namespace.clone(),
    pod_name: pod.name.clone(),
    message: pod_status_message(pod),
    age: pod.age.clone(),
  }))
}

// ---------------------------------------------------------------------------
// Registry of all pod checks
// ---------------------------------------------------------------------------

/// Returns all registered pod checks. Add new checks here.
fn all_pod_checks() -> Vec<PodCheck> {
  vec![check_pod_phase]
}

// ---------------------------------------------------------------------------
// Pod evaluation entry point
// ---------------------------------------------------------------------------

/// Run every registered pod check against every pod and return the flattened
/// display findings.
pub fn evaluate_pod_findings(pods: &[KubePod]) -> Vec<DisplayFinding> {
  let checks = all_pod_checks();

  pods
    .iter()
    .flat_map(|pod| {
      checks
        .iter()
        .filter_map(move |check| check(pod).map(|f| f.into_display_finding()))
    })
    .collect()
}
