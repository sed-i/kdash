use async_trait::async_trait;
use k8s_openapi::{api::core::v1::Event, chrono::Utc};
use ratatui::{
  layout::{Constraint, Rect},
  widgets::{Cell, Row},
  Frame,
};

use super::{
  models::{AppResource, KubeResource},
  utils, ActiveBlock, App,
};
use crate::{
  draw_resource_tab,
  network::Network,
  ui::utils::{
    draw_describe_block, draw_resource_block, draw_yaml_block, get_describe_active,
    get_resource_title, style_primary, title_with_dual_style, ResourceTableProps, COPY_HINT,
    DESCRIBE_YAML_AND_ESC_HINT,
  },
};

#[derive(Clone, Debug, PartialEq)]
pub struct KubeEvent {
  pub name: String,
  pub namespace: String,
  pub involved_kind: String,
  pub reason: String,
  pub message: String,
  pub count: i32,
  pub age: String,
  k8s_obj: Event,
}

impl From<Event> for KubeEvent {
  fn from(event: Event) -> Self {
    KubeEvent {
      name: event.metadata.name.clone().unwrap_or_default(),
      namespace: event.metadata.namespace.clone().unwrap_or_default(),
      involved_kind: event.involved_object.kind.clone().unwrap_or_default(),
      reason: event.reason.clone().unwrap_or_default(),
      message: event.message.clone().unwrap_or_default(),
      count: event.count.unwrap_or_default(),
      age: utils::to_age(event.metadata.creation_timestamp.as_ref(), Utc::now()),
      k8s_obj: utils::sanitize_obj(event),
    }
  }
}

impl KubeResource<Event> for KubeEvent {
  fn get_name(&self) -> &String {
    &self.name
  }
  fn get_k8s_obj(&self) -> &Event {
    &self.k8s_obj
  }
}

static EVENTS_TITLE: &str = "Events";

pub struct EventResource {}

#[async_trait]
impl AppResource for EventResource {
  fn render(block: ActiveBlock, f: &mut Frame<'_>, app: &mut App, area: Rect) {
    draw_resource_tab!(
      EVENTS_TITLE,
      block,
      f,
      app,
      area,
      Self::render,
      draw_block,
      app.data.events
    );
  }

  async fn get_resource(nw: &Network<'_>) {
    let items: Vec<KubeEvent> = nw.get_namespaced_resources(Event::into).await;

    let mut app = nw.app.lock().await;
    app.data.events.set_items(items);
  }
}

fn draw_block(f: &mut Frame<'_>, app: &mut App, area: Rect) {
  let title = get_resource_title(app, EVENTS_TITLE, "", app.data.events.items.len());

  draw_resource_block(
    f,
    area,
    ResourceTableProps {
      title,
      inline_help: DESCRIBE_YAML_AND_ESC_HINT.into(),
      resource: &mut app.data.events,
      table_headers: vec![
        "Namespace",
        "Name",
        "Involved Kind",
        "Reason",
        "Message",
        "Count",
        "Age",
      ],
      column_widths: vec![
        Constraint::Percentage(12),
        Constraint::Percentage(18),
        Constraint::Percentage(12),
        Constraint::Percentage(13),
        Constraint::Percentage(30),
        Constraint::Percentage(5),
        Constraint::Percentage(10),
      ],
    },
    |c| {
      Row::new(vec![
        Cell::from(c.namespace.to_owned()),
        Cell::from(c.name.to_owned()),
        Cell::from(c.involved_kind.to_owned()),
        Cell::from(c.reason.to_owned()),
        Cell::from(c.message.to_owned()),
        Cell::from(c.count.to_string()),
        Cell::from(c.age.to_owned()),
      ])
      .style(style_primary(app.light_theme))
    },
    app.light_theme,
    app.is_loading,
    app.data.selected.filter.to_owned(),
  );
}

#[cfg(test)]
mod tests {
  use k8s_openapi::{
    api::core::v1::ObjectReference, apimachinery::pkg::apis::meta::v1::ObjectMeta, chrono::Utc,
  };

  use super::*;
  use crate::app::test_utils::get_time;

  #[test]
  fn test_event_from_api() {
    let event = Event {
      metadata: ObjectMeta {
        name: Some("pod.123".into()),
        namespace: Some("default".into()),
        creation_timestamp: Some(get_time("2023-06-30T17:27:23Z")),
        ..Default::default()
      },
      involved_object: ObjectReference {
        kind: Some("Pod".into()),
        name: Some("pod-1".into()),
        ..Default::default()
      },
      reason: Some("Scheduled".into()),
      message: Some("Successfully assigned".into()),
      count: Some(3),
      ..Default::default()
    };

    assert_eq!(
      KubeEvent::from(event.clone()),
      KubeEvent {
        name: "pod.123".into(),
        namespace: "default".into(),
        involved_kind: "Pod".into(),
        reason: "Scheduled".into(),
        message: "Successfully assigned".into(),
        count: 3,
        age: utils::to_age(Some(&get_time("2023-06-30T17:27:23Z")), Utc::now()),
        k8s_obj: utils::sanitize_obj(event),
      }
    );
  }
}
