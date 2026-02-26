use async_trait::async_trait;
use ratatui::{
  layout::{Constraint, Rect},
  widgets::{Cell, Row},
  Frame,
};
use strum::Display;

use super::{
  models::{AppResource, KubeResource, StatefulTable},
  troubleshoot_rules, ActiveBlock, App,
};
use crate::ui::utils::{
  draw_describe_block, draw_resource_block, get_resource_title, style_failure, style_primary,
  style_warning, title_with_dual_style, ResourceTableProps, COPY_HINT,
};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Display, Eq, PartialEq)]
#[allow(dead_code)]
pub enum Severity {
  Critical,
  Warn,
  Info,
}

#[derive(Clone, Copy, Debug, Display, Eq, PartialEq)]
#[allow(dead_code)]
pub enum Category {
  Workloads,
  Nodes,
  Storage,
  Networking,
}

#[derive(Clone, Copy, Debug, Display, Eq, PartialEq)]
pub enum Scope {
  Cluster,
  Namespace,
}

#[derive(Clone, Copy, Debug, Display, Eq, PartialEq)]
pub enum ResourceKind {
  Pod,
  Deployment,
  Node,
  #[strum(serialize = "PVC")]
  Pvc,
}

// ---------------------------------------------------------------------------
// Finding — a single detected issue
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
  pub id: String,
  pub title: String,
  pub severity: Severity,
  pub category: Category,
  pub scope: Scope,
  pub namespace: Option<String>,
  pub resource_kind: ResourceKind,
  pub resource_name: String,
  pub message: String,
  pub age: String,
  pub describe_kind: String,
  pub describe_name: String,
  pub describe_namespace: Option<String>,
  pub(crate) k8s_obj: (),
}

impl Finding {
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

impl KubeResource<()> for Finding {
  fn get_name(&self) -> &String {
    &self.resource_name
  }

  fn get_k8s_obj(&self) -> &() {
    &self.k8s_obj
  }
}

// ---------------------------------------------------------------------------
// Rule & Ruleset — the embedded DSL building blocks
// ---------------------------------------------------------------------------

#[derive(Clone)]
#[allow(dead_code)]
pub struct Rule {
  pub id: &'static str,
  pub title: &'static str,
  pub severity: Severity,
  pub category: Category,
  pub scope: Scope,
  pub resources: Vec<ResourceKind>,
  pub enabled: bool,
  pub evaluate: fn(&App) -> Vec<Finding>,
}

impl Rule {
  pub fn new(
    id: &'static str,
    title: &'static str,
    severity: Severity,
    category: Category,
    scope: Scope,
    resources: Vec<ResourceKind>,
    evaluate: fn(&App) -> Vec<Finding>,
  ) -> Self {
    Self {
      id,
      title,
      severity,
      category,
      scope,
      resources,
      enabled: true,
      evaluate,
    }
  }
}

#[allow(dead_code)]
pub struct Ruleset {
  pub id: &'static str,
  pub title: &'static str,
  pub description: &'static str,
  pub enabled: bool,
  pub rules: Vec<Rule>,
}

impl Ruleset {
  pub fn new(id: &'static str, title: &'static str, description: &'static str) -> Self {
    Self {
      id,
      title,
      description,
      enabled: true,
      rules: vec![],
    }
  }

  pub fn rule(mut self, rule: Rule) -> Self {
    self.rules.push(rule);
    self
  }

  pub fn evaluate(&self, app: &App) -> Vec<Finding> {
    if !self.enabled {
      return vec![];
    }
    self
      .rules
      .iter()
      .filter(|r| r.enabled)
      .flat_map(|r| (r.evaluate)(app))
      .collect()
  }
}

// ---------------------------------------------------------------------------
// Evaluation orchestrator
// ---------------------------------------------------------------------------

pub fn evaluate_findings(app: &App) -> Vec<Finding> {
  let mut findings: Vec<Finding> = troubleshoot_rules::built_in_rulesets()
    .into_iter()
    .flat_map(|rs| rs.evaluate(app))
    .collect();

  findings.sort_by(|a, b| {
    severity_rank(&a.severity)
      .cmp(&severity_rank(&b.severity))
      .then_with(|| a.category.to_string().cmp(&b.category.to_string()))
      .then_with(|| a.resource_name.cmp(&b.resource_name))
  });

  findings
}

fn severity_rank(severity: &Severity) -> u8 {
  match severity {
    Severity::Critical => 0,
    Severity::Warn => 1,
    Severity::Info => 2,
  }
}

#[allow(dead_code)]
pub fn findings_count(app: &App) -> usize {
  evaluate_findings(app).len()
}

#[allow(dead_code)]
pub fn update_findings(app: &App, table: &mut StatefulTable<Finding>) {
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
      inline_help: "| describe <d> | refresh <ctrl+r> ".into(),
      resource: findings,
      table_headers: vec![
        "Severity", "Category", "Scope", "Resource", "Message", "Age",
      ],
      column_widths: vec![
        Constraint::Percentage(10),
        Constraint::Percentage(15),
        Constraint::Percentage(10),
        Constraint::Percentage(20),
        Constraint::Percentage(35),
        Constraint::Percentage(10),
      ],
    },
    |c| {
      let style = match c.severity {
        Severity::Critical => style_failure(light_theme),
        Severity::Warn => style_warning(light_theme),
        Severity::Info => style_primary(light_theme),
      };

      Row::new(vec![
        Cell::from(c.severity.to_string()),
        Cell::from(c.category.to_string()),
        Cell::from(c.scope.to_string()),
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
            "-> Describe",
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
