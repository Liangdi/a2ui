//! Native AccessKit metadata. Hidden affects assistive technology, not visual layout.
//!
//! Input's native textbox description cannot currently be customized through
//! GPUI Kit's public Input API. A group description is not a substitute.
use a2ui_base::model::{component_context::ComponentContext, component_model::ComponentModel};
use a2ui_base::protocol::common_types::LiveRegion;
use gpui_kit::accesskit::{Live, Node};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Interactivity, IntoElement, RenderOnce, Role, SharedString, Window, div,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attributes {
    pub label: Option<String>,
    pub description: Option<String>,
    pub live: Option<LiveRegion>,
    pub hidden: Option<bool>,
}

pub fn resolve(ctx: &ComponentContext, model: &ComponentModel) -> Attributes {
    let Some(attributes) = model.accessibility() else {
        return Attributes::default();
    };
    Attributes {
        label: attributes
            .label
            .map(|v| ctx.data_context.resolve_dynamic_string(&v)),
        description: attributes
            .description
            .map(|v| ctx.data_context.resolve_dynamic_string(&v)),
        live: attributes.live,
        hidden: attributes
            .hidden
            .map(|v| ctx.data_context.resolve_dynamic_boolean(&v)),
    }
}

pub fn label(ctx: &ComponentContext, model: &ComponentModel) -> Option<String> {
    resolve(ctx, model).label
}

/// Apply to the actual node, after the widget has supplied its default semantics.
/// Keeping this shared with tests verifies the exact AccessKit properties written.
pub fn apply_to_node(node: &mut Node, attributes: &Attributes) {
    if let Some(label) = &attributes.label {
        node.set_label(label.clone());
    }
    if let Some(description) = &attributes.description {
        node.set_description(description.clone());
    }
    if let Some(live) = attributes.live {
        node.set_live(match live {
            LiveRegion::Off => Live::Off,
            LiveRegion::Polite => Live::Polite,
            LiveRegion::Assertive => Live::Assertive,
        });
    }
    if attributes.hidden == Some(true) {
        node.set_hidden();
    } else if attributes.hidden == Some(false) {
        node.clear_hidden();
    }
}

/// A transparent facade enabling stateful accessibility builders on widgets
/// (notably styled Button) whose public API only implements InteractiveElement.
#[derive(IntoElement)]
pub struct Semantic<T: IntoElement + 'static> {
    inner: T,
}
impl<T: IntoElement + 'static> RenderOnce for Semantic<T> {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.inner
    }
}
impl<T: InteractiveElement + IntoElement + 'static> InteractiveElement for Semantic<T> {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.inner.interactivity()
    }
}
impl<T: InteractiveElement + IntoElement + 'static> StatefulInteractiveElement for Semantic<T> {}

/// Button/Checkbox retain their native role, focus, checked state and activation.
pub fn interactive<T: InteractiveElement + IntoElement + 'static>(
    element: T,
    ctx: &ComponentContext,
    model: &ComponentModel,
) -> Semantic<T> {
    let attributes = resolve(ctx, model);
    Semantic { inner: element }.a11y_synthetic_children(move |builder| {
        apply_to_node(builder.parent_node(), &attributes);
    })
}

/// Container/content metadata and hidden subtree inheritance. Interactive widget
/// names/descriptions belong on their own native node, not this enclosing group.
pub fn wrap(ctx: &ComponentContext, model: &ComponentModel, child: AnyElement) -> AnyElement {
    let mut attributes = resolve(ctx, model);
    let interactive = matches!(
        model.component_type.as_str(),
        "Button" | "CheckBox" | "TextField" | "ChoicePicker" | "Slider" | "DateTimeInput"
    );
    if interactive {
        attributes.label = None;
        attributes.description = None;
    }
    if attributes == Attributes::default() {
        return child;
    }
    let key = serde_json::to_string(&(
        &ctx.surface_id,
        &ctx.component_id,
        ctx.data_context.base_path(),
        "accessibility",
    ))
    .unwrap();
    let role = if model.component_type == "Text" {
        Role::Label
    } else {
        Role::Group
    };
    if attributes.label.is_none() && role == Role::Label {
        attributes.label = model
            .get_property("text")
            .map(|text| ctx.data_context.resolve_dynamic_string(&text));
    }
    div()
        .id(SharedString::from(key))
        .role(role)
        .a11y_synthetic_children(move |builder| apply_to_node(builder.parent_node(), &attributes))
        .child(child)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_updates_actual_accesskit_properties() {
        let mut node = Node::new(Role::Button);
        node.set_label("Visible text");
        apply_to_node(
            &mut node,
            &Attributes {
                label: Some("Mute Notifications".into()),
                description: Some("Conversation notifications".into()),
                live: Some(LiveRegion::Assertive),
                hidden: Some(true),
            },
        );
        assert_eq!(node.role(), Role::Button);
        assert_eq!(node.label(), Some("Mute Notifications"));
        assert_eq!(node.description(), Some("Conversation notifications"));
        assert_eq!(node.live(), Some(Live::Assertive));
        assert!(node.is_hidden());
        apply_to_node(
            &mut node,
            &Attributes {
                hidden: Some(false),
                live: Some(LiveRegion::Off),
                ..Default::default()
            },
        );
        assert!(!node.is_hidden());
        assert_eq!(node.live(), Some(Live::Off));
    }
}

#[cfg(test)]
mod official_metadata_tests {
    use super::*;
    use a2ui_base::model::{components_model::SurfaceComponentsModel, data_model::DataModel};
    use serde_json::Value;
    use std::collections::HashMap;

    // Deliberately a metadata projection audit, not a native renderer conformance claim.
    #[test]
    fn official_seven_metadata_cases_including_dynamic_updates() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../tests/fixtures/accessibility.json")).unwrap();
        assert_eq!(cases.len(), 7);
        let mut checked = 0;
        for case in cases {
            let mut data = DataModel::new();
            let mut components = SurfaceComponentsModel::new();
            let stages = case
                .get("steps")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_else(|| vec![case.clone()]);
            for stage in stages {
                for message in stage["messages"].as_array().unwrap() {
                    if let Some(create) = message.get("createSurface") {
                        data.replace_all(
                            create
                                .get("dataModel")
                                .cloned()
                                .unwrap_or(serde_json::json!({})),
                        );
                        for component in create["components"].as_array().unwrap() {
                            components.upsert(ComponentModel::from_json(component).unwrap());
                        }
                    }
                    if let Some(update) = message.get("updateDataModel") {
                        data.set(update["path"].as_str().unwrap(), update["value"].clone());
                    }
                }
                for (id, expected) in stage["assertions"]["accessibilityTree"]
                    .as_object()
                    .unwrap()
                {
                    let model = components.get(id).unwrap();
                    let functions = HashMap::new();
                    let ctx = ComponentContext::new(
                        id.clone(),
                        "a11y_surface".into(),
                        &data,
                        &components,
                        &functions,
                        "",
                        None,
                    );
                    let attributes = resolve(&ctx, model);
                    let mut node = Node::new(Role::Group);
                    apply_to_node(&mut node, &attributes);
                    // Explicit metadata only. Inferred names/roles/checked belong to
                    // native widgets and are intentionally not manufactured here.
                    if let (Some(actual), Some(expected)) =
                        (attributes.label.as_deref(), expected.get("label"))
                    {
                        assert_eq!(node.label(), Some(actual));
                        assert_eq!(Some(actual), expected.as_str());
                        checked += 1;
                    }
                    if let Some(expected) = expected.get("description") {
                        assert_eq!(node.description(), expected.as_str());
                        checked += 1;
                    }
                    if let Some(expected) = expected.get("hidden") {
                        assert_eq!(node.is_hidden(), expected.as_bool().unwrap());
                        checked += 1;
                    }
                    if let Some(expected) = expected.get("live") {
                        let live = match expected.as_str().unwrap() {
                            "polite" => Live::Polite,
                            "assertive" => Live::Assertive,
                            _ => Live::Off,
                        };
                        assert_eq!(node.live(), Some(live));
                        checked += 1;
                    }
                }
            }
        }
        assert!(
            checked >= 15,
            "ensure meaningful official metadata coverage"
        );
    }
}
