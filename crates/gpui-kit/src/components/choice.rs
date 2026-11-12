//! ChoicePicker semantics, option identity and persistent native selection state.
//! Kept separate from shared traversal and other widget lifecycles.
use super::{Walk, instance_key, update_config};
use crate::{app::GpuiApp, theme};
use a2ui_base::model::{component_context::ComponentContext, component_model::ComponentModel};
use a2ui_base::protocol::common_types::{DynamicString, DynamicStringList, DynamicValue};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::searchable_list::SearchableListItem;
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectState};
use gpui_kit::component::{Disableable as _, IndexPath, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, SharedString, div, px};
use serde_json::Value;

/// Keep the protocol value separate from its human-readable label.
#[derive(Clone)]
pub(crate) struct ChoiceOption {
    label: String,
    value: String,
}

impl SearchableListItem for ChoiceOption {
    type Value = String;
    fn title(&self) -> SharedString {
        self.label.clone().into()
    }
    fn value(&self) -> &String {
        &self.value
    }
}

/// ChoicePicker — a list of selectable options.
///
/// - Single selection renders a native [`Select`] dropdown; choosing an option
///   writes back `json!([value])` (an array, matching the TUI/iced backends).
/// - Multiple selection renders a column of native checkboxes; toggling
///   adds/removes the value in the array written back.
///
/// Only a `Binding` `value` is writable; a non-binding value degrades to a
/// read-only control (mirrors the TUI `handle_event` bail-out).
pub(crate) fn render_choice_picker(
    walk: &Walk,
    ctx: &ComponentContext,
    model: &ComponentModel,
    window: &mut gpui_kit::Window,
    cx: &mut Context<GpuiApp>,
) -> AnyElement {
    let label = model
        .get_property::<DynamicString>("label")
        .map(|ds| ctx.data_context.resolve_dynamic_string(&ds))
        .unwrap_or_default();
    let accessible_name = choice_accessible_name(crate::accessibility::label(ctx, model), &label);
    let options = read_options(model);
    let value_binding = model.get_property::<DynamicStringList>("value");
    let selected_values = value_binding
        .as_ref()
        .map(|dsl| resolve_choice_value(ctx, dsl))
        .unwrap_or_default();
    let path: Option<String> = match &value_binding {
        Some(DynamicStringList::Binding(b)) => Some(ctx.data_context.resolve_pointer(&b.path)),
        _ => None,
    };
    let is_multiple = model
        .get_property::<String>("variant")
        .as_deref()
        .map(|v| v == "multipleSelection")
        .unwrap_or(false);

    let mut col = v_flex().gap_1p5().w_full();
    if !label.is_empty() {
        col = col.child(
            div()
                .text_color(theme::SUBTEXT0)
                .text_size(px(12.))
                .child(label),
        );
    }
    if options.is_empty() {
        return col.into_any_element();
    }

    if is_multiple {
        for (opt_label, opt_value) in options {
            let checked = selected_values.contains(&opt_value);
            let cb_id = serde_json::to_string(&(instance_key(ctx), &opt_value)).unwrap();
            let mut cb = Checkbox::new(SharedString::from(cb_id))
                .checked(checked)
                .disabled(path.is_none())
                .label(opt_label);
            if let Some(p) = &path {
                let path = p.clone();
                let selected = selected_values.clone();
                let value = opt_value.clone();
                cb = cb.on_click(cx.listener(move |this, now_checked: &bool, _window, cx| {
                    let mut next = selected.clone();
                    if *now_checked {
                        if !next.contains(&value) {
                            next.push(value.clone());
                        }
                    } else {
                        next.retain(|v| v != &value);
                    }
                    this.set_data_value(&path, serde_json::json!(next));
                    cx.notify();
                }));
            }
            col = col.child(cb);
        }
    } else {
        let read_only = path.is_none();
        let state = ensure_select_state(
            walk,
            &instance_key(ctx),
            options,
            selected_values,
            path,
            window,
            cx,
        );
        let mut select = Select::new(&state).disabled(read_only).w_full();
        if let Some(name) = accessible_name.clone() {
            select = select.accessibility_label(name);
        }
        col = col.child(select);
    }

    if is_multiple {
        let mut attributes = crate::accessibility::resolve(ctx, model);
        attributes.label = accessible_name;
        return col
            .id(SharedString::from(format!(
                "choice-group:{}",
                instance_key(ctx)
            )))
            .role(gpui_kit::Role::Group)
            .a11y_synthetic_children(move |builder| {
                crate::accessibility::apply_to_node(builder.parent_node(), &attributes);
            })
            .into_any_element();
    }
    col.into_any_element()
}

fn choice_accessible_name(explicit: Option<String>, visible: &str) -> Option<String> {
    explicit.or_else(|| (!visible.is_empty()).then(|| visible.to_owned()))
}

#[cfg(test)]
mod choice_accessibility_tests {
    use super::choice_accessible_name;
    #[test]
    fn explicit_names_override_visible_label_not_selected_value() {
        assert_eq!(
            choice_accessible_name(Some("Shipping speed".into()), "Speed"),
            Some("Shipping speed".into())
        );
        assert_eq!(choice_accessible_name(None, "Speed"), Some("Speed".into()));
        assert_eq!(choice_accessible_name(None, ""), None);
        assert_eq!(
            choice_accessible_name(Some("".into()), "Speed"),
            Some("".into())
        );
    }
}

/// Lazily create (or fetch) the single-select [`SelectState`] for `id`,
/// populating option labels, the selected index, and subscribing for write-back
/// of `json!([value])` on confirm.
#[allow(clippy::too_many_arguments)]
fn ensure_select_state(
    walk: &Walk,
    id: &str,
    options: Vec<(String, String)>,
    selected_values: Vec<String>,
    path: Option<String>,
    window: &mut gpui_kit::Window,
    cx: &mut Context<GpuiApp>,
) -> Entity<SelectState<SearchableVec<ChoiceOption>>> {
    let items: Vec<ChoiceOption> = options
        .iter()
        .map(|(label, value)| ChoiceOption {
            label: label.clone(),
            value: value.clone(),
        })
        .collect();
    let selected_index = selected_values.first().and_then(|v| {
        options
            .iter()
            .position(|(_, val)| val == v)
            .map(IndexPath::new)
    });

    let config_key = format!("select:{id}");
    let changed = update_config(walk.state_configs, &config_key, serde_json::json!(path));
    let items_changed = update_config(
        walk.state_configs,
        &format!("items:{config_key}"),
        serde_json::json!(options),
    );
    let value_changed = update_config(
        walk.state_configs,
        &format!("value:{config_key}"),
        serde_json::json!(selected_values),
    );
    if !changed && let Some(existing) = walk.select_states.borrow().get(id).cloned() {
        if items_changed || value_changed {
            existing.update(cx, |state, cx| {
                if items_changed {
                    state.set_items(SearchableVec::new(items), window, cx);
                }
                state.set_selected_index(selected_index, window, cx);
            });
        }
        return existing;
    }
    let delegate = SearchableVec::new(items);
    let new_state = cx.new(|cx| SelectState::new(delegate, selected_index, window, cx));

    let sub = cx.subscribe(
        &new_state,
        move |this: &mut GpuiApp,
              _src: Entity<SelectState<SearchableVec<ChoiceOption>>>,
              ev: &SelectEvent<SearchableVec<ChoiceOption>>,
              cx: &mut Context<GpuiApp>| {
            if let SelectEvent::Confirm(Some(picked)) = ev
                && let Some(path) = &path
            {
                this.set_data_value(path, serde_json::json!([picked]));
                cx.notify();
            }
        },
    );
    walk.subscriptions.borrow_mut().insert(config_key, sub);
    walk.select_states
        .borrow_mut()
        .insert(id.to_string(), new_state.clone());
    new_state
}

/// Read a ChoicePicker's `options` array into `(label, value)` pairs.
fn read_options(model: &ComponentModel) -> Vec<(String, String)> {
    let Some(arr) = model.get_raw("options").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_choice_option).collect()
}

/// Parse one `{label, value}` option (`value` optional, defaults to empty).
fn parse_choice_option(v: &Value) -> Option<(String, String)> {
    let label = v.get("label")?.as_str()?.to_string();
    let value = v
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some((label, value))
}

/// Resolve a ChoicePicker's current selection from its `value` `DynamicStringList`
/// (mirrors iced).
fn resolve_choice_value(ctx: &ComponentContext, dsl: &DynamicStringList) -> Vec<String> {
    match dsl {
        DynamicStringList::Literal(v) => v.clone(),
        DynamicStringList::Binding(b) => match ctx.data_context.get(&b.path) {
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            Some(Value::String(s)) => vec![s],
            _ => Vec::new(),
        },
        DynamicStringList::Function(fc) => match ctx
            .data_context
            .resolve_dynamic_value(&DynamicValue::Function(fc.clone()))
        {
            Value::Array(arr) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            Value::String(s) => vec![s],
            _ => Vec::new(),
        },
    }
}

#[cfg(test)]
mod option_tests {
    use super::*;
    #[test]
    fn choice_labels_do_not_replace_values() {
        let first = ChoiceOption {
            label: "English".into(),
            value: "en".into(),
        };
        let duplicate_label = ChoiceOption {
            label: "English".into(),
            value: "en-GB".into(),
        };
        assert_eq!(first.title().as_ref(), "English");
        assert_eq!(first.value(), "en");
        assert_eq!(duplicate_label.value(), "en-GB");
    }
    #[test]
    fn parse_choice_option_defaults_value_to_empty() {
        let v = serde_json::json!({ "label": "Code" });
        let (label, value) = parse_choice_option(&v).expect("valid option");
        assert_eq!(label, "Code");
        assert_eq!(value, "");
    }
}
