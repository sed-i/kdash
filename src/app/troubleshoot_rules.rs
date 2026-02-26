use phf::phf_map;

use super::{
  models::KubeResource,
  troubleshoot::{Category, Finding, ResourceKind, Rule, Ruleset, Scope, Severity},
  App,
};

// ---------------------------------------------------------------------------
// Finding metadata — the value side of each phf_map entry.
// The map key itself carries the phase/status string, so it is not repeated
// inside the struct.
// ---------------------------------------------------------------------------

struct FindingMeta {
  id: &'static str,
  severity: Severity,
}

// ---------------------------------------------------------------------------
// Phase / status → FindingMeta maps (compile-time perfect hash)
// ---------------------------------------------------------------------------

/// Unhealthy pod phases per the Pod API.
///
/// The five possible values for `pod.status.phase` are:
///   Pending, Running, Succeeded, Failed, Unknown
///
/// Running and Succeeded are healthy and are not listed here.
///
/// References:
/// - https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#pod-phase
/// - https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.35/#podstatus-v1-core
static POD_PHASE_FINDINGS: phf::Map<&'static str, FindingMeta> = phf_map! {
  "Failed" => FindingMeta {
    id: "pod.phase.failed",
    severity: Severity::Critical,
  },
  "Unknown" => FindingMeta {
    id: "pod.phase.unknown",
    severity: Severity::Warn,
  },
  "Pending" => FindingMeta {
    id: "pod.phase.pending",
    severity: Severity::Info,
  },
};

// ---------------------------------------------------------------------------
// Ruleset registration
// ---------------------------------------------------------------------------

pub fn built_in_rulesets() -> Vec<Ruleset> {
  vec![Ruleset::new(
    "kdash.core.workloads",
    "Workload Health",
    "Workload readiness and stability checks",
  )
  .rule(Rule::new(
    "pod.phase",
    "Pod unhealthy phase",
    Severity::Critical,
    Category::Workloads,
    Scope::Namespace,
    vec![ResourceKind::Pod],
    eval_pod_phase,
  ))]
}

// ---------------------------------------------------------------------------
// Pod phase eval — uses pod.status.phase from the Kubernetes API, NOT the
// synthesized display status that kubectl (and kdash) show to the user.
// ---------------------------------------------------------------------------

fn eval_pod_phase(app: &App) -> Vec<Finding> {
  app
    .data
    .pods
    .items
    .iter()
    .filter_map(|p| {
      let phase = p
        .get_k8s_obj()
        .status
        .as_ref()
        .and_then(|s| s.phase.as_deref())
        .unwrap_or("Unknown");
      let reason = p
        .get_k8s_obj()
        .status
        .as_ref()
        .and_then(|s| s.reason.as_deref())
        .unwrap_or("Unknown");
      let message = p
        .get_k8s_obj()
        .status
        .as_ref()
        .and_then(|s| s.message.as_deref())
        .unwrap_or("Unknown");

      let meta = POD_PHASE_FINDINGS.get(phase)?;
      Some(Finding {
        id: meta.id.into(),
        title: reason.into(),
        severity: meta.severity,
        category: Category::Workloads,
        scope: Scope::Namespace,
        namespace: Some(p.namespace.clone()),
        resource_kind: ResourceKind::Pod,
        resource_name: p.name.clone(),
        message: message.into(),
        age: p.age.clone(),
        describe_kind: "pod".into(),
        describe_name: p.name.clone(),
        describe_namespace: Some(p.namespace.clone()),
        k8s_obj: (),
      })
    })
    .collect()
}
