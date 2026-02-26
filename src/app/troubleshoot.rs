use async_trait::async_trait;
use ratatui::{
  layout::{Constraint, Rect},
  widgets::{Cell, Row},
  Frame,
};
use strum::Display;

use super::{
  models::{AppResource, KubeResource, StatefulTable},
  troubleshoot_pod, ActiveBlock, App,
};
use crate::ui::utils::{
  draw_describe_block, draw_resource_block, draw_yaml_block, get_describe_active,
  get_resource_title, style_failure, style_primary, style_warning, title_with_dual_style,
  ResourceTableProps, COPY_HINT, DESCRIBE_AND_YAML_HINT,
};

// ---------------------------------------------------------------------------
// Core generic finding type
// ---------------------------------------------------------------------------

/// A finding produced by a troubleshooting check.
///
/// The variant encodes the severity while `R` carries resource-specific data
/// (e.g. `PodFinding`).
///
/// Variant declaration order encodes sort priority via derived `Ord`:
/// `Error` (most severe) < `Warn` < `Info` (least severe), so a normal
/// ascending sort puts errors first.
#[derive(Clone, Debug, Display, Eq, Ord, PartialEq, PartialOrd)]
pub enum Finding<R> {
  Error(R),
  Warn(R),
  Info(R),
}

#[allow(dead_code)]
impl<R> Finding<R> {
  /// Returns a data-less copy that preserves only the severity variant.
  /// Useful for storing in type-erased contexts like `DisplayFinding`.
  pub fn severity_tag(&self) -> Finding<()> {
    match self {
      Finding::Error(_) => Finding::Error(()),
      Finding::Warn(_) => Finding::Warn(()),
      Finding::Info(_) => Finding::Info(()),
    }
  }

  /// Returns a reference to the inner resource finding.
  pub fn inner(&self) -> &R {
    match self {
      Finding::Info(r) | Finding::Warn(r) | Finding::Error(r) => r,
    }
  }

  /// Consumes the finding and returns the inner resource finding.
  pub fn into_inner(self) -> R {
    match self {
      Finding::Info(r) | Finding::Warn(r) | Finding::Error(r) => r,
    }
  }
}

// ---------------------------------------------------------------------------
// Display enums shared across resource-specific findings
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Display, Eq, PartialEq)]
pub enum ResourceKind {
  Pod,
  // Deployment,
  // Node,
  // #[strum(serialize = "PVC")]
  // Pvc,
}

// ---------------------------------------------------------------------------
// DisplayFinding — the concrete, type-erased row stored in StatefulTable
// ---------------------------------------------------------------------------

/// A flattened, UI-ready representation of any resource finding.
///
/// Resource-specific `Finding<R>` values are converted into this type via
/// the [`IntoDisplayFinding`] trait so they can be stored in a single
/// homogeneous `StatefulTable<DisplayFinding>`.
///
/// The `severity` field is a `Finding<()>` — the same `Finding` enum with
/// no payload — which gives us `Ord` for sorting and `Display` for rendering
/// without introducing a redundant severity type.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayFinding {
  pub severity: Finding<()>,
  pub reason: String,
  pub resource_kind: ResourceKind,
  pub namespace: Option<String>,
  pub resource_name: String,
  pub message: String,
  pub age: String,
  pub describe_kind: String,
  pub describe_name: String,
  pub describe_namespace: Option<String>,
  // Unit k8s_obj kept for KubeResource trait compatibility
  pub(crate) k8s_obj: (),
}

impl DisplayFinding {
  pub fn resource_ref(&self) -> String {
    match &self.namespace {
      Some(ns) if !ns.is_empty() => format!("{}/{}", ns, self.resource_name),
      _ => self.resource_name.clone(),
    }
  }

  pub fn describe_target(&self) -> (&str, &str, Option<&str>) {
    (
      self.describe_kind.as_str(),
      self.describe_name.as_str(),
      self.describe_namespace.as_deref(),
    )
  }
}

impl KubeResource<()> for DisplayFinding {
  fn get_name(&self) -> &String {
    &self.resource_name
  }

  fn get_k8s_obj(&self) -> &() {
    &self.k8s_obj
  }
}

// ---------------------------------------------------------------------------
// Conversion trait — resource findings → display findings
// ---------------------------------------------------------------------------

/// Implement this trait on `Finding<R>` for each resource-specific finding
/// type `R` to enable conversion into `DisplayFinding`.
pub trait IntoDisplayFinding {
  fn into_display_finding(self) -> DisplayFinding;
}

// ---------------------------------------------------------------------------
// Evaluation orchestrator
// ---------------------------------------------------------------------------

pub fn evaluate_findings(app: &App) -> Vec<DisplayFinding> {
  let mut findings: Vec<DisplayFinding> = Vec::new();

  // Collect pod findings
  findings.extend(troubleshoot_pod::evaluate_pod_findings(
    &app.data.pods.items,
  ));

  // Future: findings.extend(troubleshoot_node::evaluate_node_findings(...));
  // Future: findings.extend(troubleshoot_deployment::evaluate_deployment_findings(...));

  findings.sort_by(|a, b| {
    a.severity
      .cmp(&b.severity)
      .then_with(|| a.resource_name.cmp(&b.resource_name))
  });

  findings
}

#[allow(dead_code)]
pub fn findings_count(app: &App) -> usize {
  evaluate_findings(app).len()
}

#[allow(dead_code)]
pub fn update_findings(app: &App, table: &mut StatefulTable<DisplayFinding>) {
  let items = evaluate_findings(app);
  table.set_items(items);
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn render_troubleshoot(f: &mut Frame<'_>, app: &mut App, area: Rect) {
  let light_theme = app.light_theme;
  let is_loading = app.is_loading;
  let filter = app.data.selected.filter.to_owned();
  let title = get_resource_title(
    app,
    "Troubleshoot",
    "",
    app.data.troubleshoot_findings.items.len(),
  );
  let findings = &mut app.data.troubleshoot_findings;

  draw_resource_block(
    f,
    area,
    ResourceTableProps {
      title,
      inline_help: format!("{} | refresh <ctrl+r> ", DESCRIBE_AND_YAML_HINT),
      resource: findings,
      table_headers: vec!["Severity", "Reason", "Resource", "Message", "Age"],
      column_widths: vec![
        Constraint::Percentage(8),
        Constraint::Percentage(15),
        Constraint::Percentage(20),
        Constraint::Percentage(45),
        Constraint::Percentage(12),
      ],
    },
    |c| {
      let style = match c.severity {
        Finding::Error(()) => style_failure(light_theme),
        Finding::Warn(()) => style_warning(light_theme),
        Finding::Info(()) => style_primary(light_theme),
      };

      Row::new(vec![
        Cell::from(c.severity.to_string()),
        Cell::from(c.reason.clone()),
        Cell::from(format!("{} {}", c.resource_kind, c.resource_ref())),
        Cell::from(c.message.clone()),
        Cell::from(c.age.clone()),
      ])
      .style(style)
    },
    light_theme,
    is_loading,
    filter,
  );
}

pub struct TroubleshootResource;

#[async_trait]
impl AppResource for TroubleshootResource {
  fn render(block: ActiveBlock, f: &mut Frame<'_>, app: &mut App, area: Rect) {
    match block {
      ActiveBlock::Describe => draw_describe_block(
        f,
        app,
        area,
        title_with_dual_style(
          get_resource_title(
            app,
            "Troubleshoot",
            get_describe_active(block),
            app.data.troubleshoot_findings.items.len(),
          ),
          format!("{} | Troubleshoot <esc> ", COPY_HINT),
          app.light_theme,
        ),
      ),
      ActiveBlock::Yaml => draw_yaml_block(
        f,
        app,
        area,
        title_with_dual_style(
          get_resource_title(
            app,
            "Troubleshoot",
            get_describe_active(block),
            app.data.troubleshoot_findings.items.len(),
          ),
          format!("{} | Troubleshoot <esc> ", COPY_HINT),
          app.light_theme,
        ),
      ),
      _ => render_troubleshoot(f, app, area),
    }
  }

  async fn get_resource(_network: &crate::network::Network<'_>) {
    // no-op: findings are derived from already cached resources
  }
}
