//! Automatic, scope-keyed current-protocol subscriptions for the native view.
use std::collections::{HashMap, HashSet};

use a2ui_base::evaluation::{EvaluationMode, EvaluationStatus};
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::model::component_model::ComponentModel;
use a2ui_base::protocol::common_types::ChildList;
use serde_json::{Map, Value, json};

pub(crate) fn instance_key(surface: &str, component: &str, base: &str) -> String {
    json!([surface, component, base]).to_string()
}

#[derive(Default)]
pub(crate) struct ReactiveView {
    pub resolved: Map<String, Value>,
    pub pending: bool,
    pub errors: Vec<String>,
    pub checks_pending: bool,
    pub checks_valid: bool,
    pub check_messages: Vec<String>,
}

struct Subscription {
    id: String,
    expression: Value,
    check: bool,
}

#[derive(Default)]
pub(crate) struct ReactiveState {
    subscriptions: HashMap<String, Subscription>,
    views: HashMap<String, ReactiveView>,
}

impl ReactiveState {
    pub fn view(&self, surface: &str, component: &str, base: &str) -> Option<&ReactiveView> {
        self.views.get(&instance_key(surface, component, base))
    }

    pub fn refresh(&mut self, processor: &mut MessageProcessor) {
        let instances = discover(processor);
        let mut needed = HashSet::new();
        let mut views = HashMap::new();
        for (surface, base, model) in instances {
            let key = instance_key(&surface, &model.id, &base);
            let mut view = ReactiveView {
                checks_valid: true,
                ..Default::default()
            };
            // Only dynamic value properties, never actions, references, template
            // paths or writable @path bindings. Literal objects are not re-parsed.
            for name in [
                "text",
                "label",
                "url",
                "value",
                "name",
                "description",
                "min",
                "max",
                "steps",
                "activeTab",
            ] {
                let Some(expression) = model
                    .properties
                    .get(name)
                    .filter(|value| contains_call(value))
                else {
                    continue;
                };
                let sub_key = json!([key, "property", name]).to_string();
                needed.insert(sub_key.clone());
                match self.ensure(processor, &sub_key, &surface, &base, expression, false) {
                    Ok(id) => match processor.evaluation_status(&id) {
                        Some(EvaluationStatus::Ready(value)) if safe_literal(&value) => {
                            view.resolved.insert(name.into(), value);
                        }
                        Some(EvaluationStatus::Ready(_)) => view
                            .errors
                            .push(format!("{name}: expected a scalar or scalar list")),
                        Some(EvaluationStatus::Error { message, .. }) => {
                            view.errors.push(format!("{name}: {message}"))
                        }
                        _ => view.pending = true,
                    },
                    Err(error) => view.errors.push(error),
                }
            }
            if let Some(checks) = model.properties.get("@checks").and_then(Value::as_array) {
                for (index, rule) in checks.iter().enumerate() {
                    let Some(condition) = rule.get("condition") else {
                        view.checks_valid = false;
                        view.check_messages
                            .push("Invalid check: missing condition".into());
                        continue;
                    };
                    let sub_key = json!([key, "check", index]).to_string();
                    needed.insert(sub_key.clone());
                    match self.ensure(processor, &sub_key, &surface, &base, condition, true) {
                        Ok(id) => match processor.check_status(&id) {
                            Some(status) => {
                                view.checks_pending |= status.pending;
                                view.checks_valid &= status.valid;
                                if !status.valid {
                                    view.check_messages.push(
                                        status
                                            .error
                                            .map(|error| error.message)
                                            .or_else(|| {
                                                rule.get("message")
                                                    .and_then(Value::as_str)
                                                    .filter(|s| !s.is_empty())
                                                    .map(str::to_owned)
                                            })
                                            .unwrap_or_else(|| "Please check this value".into()),
                                    );
                                }
                            }
                            None => view.checks_pending = true,
                        },
                        Err(error) => {
                            view.checks_valid = false;
                            view.check_messages.push(error);
                        }
                    }
                }
            }
            views.insert(key, view);
        }
        let obsolete: Vec<_> = self
            .subscriptions
            .keys()
            .filter(|key| !needed.contains(*key))
            .cloned()
            .collect();
        for key in obsolete {
            self.remove(processor, &key);
        }
        self.views = views;
    }

    fn ensure(
        &mut self,
        processor: &mut MessageProcessor,
        key: &str,
        surface: &str,
        base: &str,
        expression: &Value,
        check: bool,
    ) -> Result<String, String> {
        if let Some(subscription) = self.subscriptions.get(key) {
            // A sample reset may keep component identity but remove core handles.
            let exists = if check {
                processor.check_status(&subscription.id).is_some()
            } else {
                processor.evaluation_status(&subscription.id).is_some()
            };
            if subscription.expression == *expression && subscription.check == check && exists {
                return Ok(subscription.id.clone());
            }
        }
        self.remove(processor, key);
        let id = if check {
            processor.start_check(surface, base, expression.clone())
        } else {
            processor.start_evaluation(surface, base, expression.clone(), EvaluationMode::Reactive)
        }
        .map_err(|error| error.to_string())?;
        self.subscriptions.insert(
            key.into(),
            Subscription {
                id: id.clone(),
                expression: expression.clone(),
                check,
            },
        );
        Ok(id)
    }

    fn remove(&mut self, processor: &mut MessageProcessor, key: &str) {
        if let Some(subscription) = self.subscriptions.remove(key) {
            if subscription.check {
                processor.cancel_check(&subscription.id);
            } else {
                processor.release_evaluation(&subscription.id);
            }
        }
    }
}

fn contains_call(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.contains_key("@call") || object.values().any(contains_call),
        Value::Array(values) => values.iter().any(contains_call),
        _ => false,
    }
}
fn safe_literal(value: &Value) -> bool {
    match value {
        Value::Object(_) => false,
        Value::Array(values) => values.iter().all(safe_literal),
        _ => true,
    }
}

fn discover(processor: &MessageProcessor) -> Vec<(String, String, ComponentModel)> {
    let Some(surface) = processor
        .model
        .surfaces()
        .next()
        .filter(|surface| surface.current_protocol)
    else {
        return Vec::new();
    };
    let components = surface.components.borrow();
    let data = surface.data_model.borrow();
    let mut stack = vec![("root".to_owned(), String::new(), 0usize)];
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    while let Some((id, base, depth)) = stack.pop() {
        if result.len() >= 512 {
            break;
        }
        if depth > 64 || !seen.insert((id.clone(), base.clone())) {
            continue;
        }
        let Some(model) = components.get(&id) else {
            continue;
        };
        // Never subscribe properties of an unsupported catalog, even when a
        // custom component happens to share a basic widget name.
        if crate::walker::check_native_catalog(processor, &surface.catalog_id, model).is_err() {
            continue;
        }
        let next = depth + 1;
        for field in ["child", "trigger", "content"] {
            if let Some(child) = model.properties.get(field).and_then(Value::as_str) {
                stack.push((child.into(), base.clone(), next));
            }
        }
        if let Some(tabs) = model.properties.get("tabs").and_then(Value::as_array) {
            for tab in tabs {
                if let Some(child) = tab.get("child").and_then(Value::as_str) {
                    stack.push((child.into(), base.clone(), next));
                }
            }
        }
        match model.children() {
            Some(ChildList::Static(children)) => {
                for child in children {
                    stack.push((child, base.clone(), next));
                }
            }
            Some(ChildList::Template { component_id, path }) => {
                let absolute = if path.starts_with('/') {
                    path
                } else {
                    format!("{}/{}", base.trim_end_matches('/'), path)
                };
                if let Some(Value::Array(items)) = data.get(&absolute) {
                    for index in (0..items.len().min(512)).rev() {
                        stack.push((component_id.clone(), format!("{absolute}/{index}"), next));
                    }
                }
            }
            None => {}
        }
        result.push((surface.id.clone(), base, model.clone()));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    const BASIC: &str = "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json";
    fn processor(components: Value, data: Value) -> MessageProcessor {
        let mut p = MessageProcessor::new(vec![a2ui_tui::catalogs::basic::build_basic_catalog()]);
        send(
            &mut p,
            json!({"version":"1.0","createSurface":{"surfaceId":"s","catalogId":BASIC,"components":components,"dataModel":data}}),
        );
        p
    }
    fn send(p: &mut MessageProcessor, message: Value) {
        p.process_message(MessageProcessor::parse_message(&message.to_string()).unwrap())
            .unwrap();
    }
    fn calls(p: &mut MessageProcessor) -> Vec<Value> {
        p.drain_outgoing()
            .into_iter()
            .map(|v| serde_json::to_value(v).unwrap()["callAgentFunction"].clone())
            .collect()
    }
    fn answer(p: &mut MessageProcessor, call: &Value, value: Value) {
        send(
            p,
            json!({"version":"1.0","agentFunctionResponse":{"functionCallId":call["functionCallId"],"value":value}}),
        );
    }
    #[test]
    fn async_property_and_check_transition_without_duplicate_requests() {
        let mut p = processor(
            json!([{"id":"root","component":"Text","text":{"@call":"fetch"},"@checks":[{"condition":{"@call":"validate"},"message":"Try again"}]}]),
            json!({}),
        );
        let mut state = ReactiveState::default();
        state.refresh(&mut p);
        let view = state.view("s", "root", "").unwrap();
        assert!(view.pending && view.checks_pending && view.checks_valid);
        let requests = calls(&mut p);
        assert_eq!(requests.len(), 2);
        state.refresh(&mut p);
        assert!(calls(&mut p).is_empty());
        for call in requests {
            answer(
                &mut p,
                &call,
                if call["callFunction"]["@call"] == "fetch" {
                    json!("Hello")
                } else {
                    json!(false)
                },
            );
        }
        state.refresh(&mut p);
        let view = state.view("s", "root", "").unwrap();
        assert!(!view.pending && !view.checks_pending && !view.checks_valid);
        assert_eq!(view.resolved["text"], "Hello");
        assert_eq!(view.check_messages, ["Try again"]);
    }
    #[test]
    fn template_instances_isolate_arguments_and_release_removed_subscriptions() {
        let mut p = processor(
            json!([
                {"id":"root","component":"Column","children":{"componentId":"row","path":"/rows"}},
                {"id":"row","component":"Text","text":{"@call":"fetch","args":{"name":{"@path":"name"}}}}
            ]),
            json!({"rows":[{"name":"first"},{"name":"second"}]}),
        );
        let mut state = ReactiveState::default();
        state.refresh(&mut p);
        let requests = calls(&mut p);
        assert_eq!(requests.len(), 2);
        for call in requests {
            let value = call["callFunction"]["args"]["name"].clone();
            answer(&mut p, &call, value);
        }
        state.refresh(&mut p);
        assert_eq!(
            state.view("s", "row", "/rows/0").unwrap().resolved["text"],
            "first"
        );
        assert_eq!(
            state.view("s", "row", "/rows/1").unwrap().resolved["text"],
            "second"
        );
        send(
            &mut p,
            json!({"version":"1.0","updateDataModel":{"surfaceId":"s","path":"/rows","value":[]}}),
        );
        state.refresh(&mut p);
        assert!(state.view("s", "row", "/rows/0").is_none());
        assert!(state.subscriptions.is_empty());
        assert!(calls(&mut p).is_empty());
    }
    #[test]
    fn explicit_native_catalog_without_default_discovers_literal_components() {
        let mut p = MessageProcessor::new(vec![a2ui_tui::catalogs::basic::build_basic_catalog()]);
        send(
            &mut p,
            json!({"version":"1.0","createSurface":{"surfaceId":"s","components":[{"id":"root","component":"Text","@catalogId":BASIC,"text":"Literal"}]}}),
        );
        let mut state = ReactiveState::default();
        state.refresh(&mut p);
        assert!(state.view("s", "root", "").is_some());
        assert!(state.subscriptions.is_empty());
        assert!(calls(&mut p).is_empty());
    }
    #[test]
    fn writable_paths_and_legacy_components_are_not_replaced() {
        let mut p = processor(
            json!([{"id":"root","component":"TextField","value":{"@path":"/name"}}]),
            json!({"name":"Max"}),
        );
        let mut state = ReactiveState::default();
        state.refresh(&mut p);
        assert!(state.view("s", "root", "").unwrap().resolved.is_empty());
        p.reset();
        send(
            &mut p,
            json!({"version":"v1.0","createSurface":{"surfaceId":"s","catalogId":BASIC}}),
        );
        send(
            &mut p,
            json!({"version":"v1.0","updateComponents":{"surfaceId":"s","components":[{"id":"root","component":"Text","text":{"call":"fetch"}}]}}),
        );
        state.refresh(&mut p);
        assert!(state.views.is_empty());
        assert!(calls(&mut p).is_empty());
    }
    #[test]
    fn property_errors_are_visible_without_replacing_source_expression() {
        let expression = json!({"@call":"fetch"});
        let mut p = processor(
            json!([{"id":"root","component":"Text","text":expression}]),
            json!({}),
        );
        let mut state = ReactiveState::default();
        state.refresh(&mut p);
        let call = calls(&mut p).remove(0);
        send(
            &mut p,
            json!({"version":"1.0","agentFunctionResponse":{"functionCallId":call["functionCallId"],"error":{"code":"FAILED","message":"Unavailable"}}}),
        );
        state.refresh(&mut p);
        let view = state.view("s", "root", "").unwrap();
        assert!(!view.pending);
        assert!(view.errors[0].contains("Unavailable"));
        assert!(view.resolved.is_empty());
        assert_eq!(
            p.model
                .get_surface("s")
                .unwrap()
                .components
                .borrow()
                .get("root")
                .unwrap()
                .properties["text"],
            expression
        );
    }
}
