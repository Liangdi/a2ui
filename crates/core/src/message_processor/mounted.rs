//! Opt-in component-property subscriptions backed by actual surface components.
use super::MessageProcessor;
use crate::{
    error::{A2uiError, Result},
    evaluation::{EvaluationMode, EvaluationStatus},
};
use serde_json::{Value, json};
use std::collections::HashMap;
#[derive(Default)]
pub(super) struct Mounted {
    bindings: HashMap<String, Binding>,
}
struct Binding {
    surface: String,
    component: String,
    property: String,
    base: String,
    check: Option<usize>,
    source: Value,
    evaluation: String,
}
impl MessageProcessor {
    /// Observe a real component property. Unmount when its host view disappears.
    pub fn mount_property(
        &mut self,
        surface: &str,
        component: &str,
        property: &str,
        base: &str,
    ) -> Result<String> {
        self.mount_binding(surface, component, property, base, None)
    }
    /// Observe a component's structured check rule, retaining message and severity.
    pub fn mount_check_rule(
        &mut self,
        surface: &str,
        component: &str,
        index: usize,
        base: &str,
    ) -> Result<String> {
        self.mount_binding(surface, component, "checks", base, Some(index))
    }
    fn mounted_source(
        &self,
        surface: &str,
        component: &str,
        property: &str,
        check: Option<usize>,
    ) -> Option<Value> {
        let surface = self.model.get_surface(surface)?;
        let components = surface.components.borrow();
        let component = components.get(component)?;
        let value = if check.is_some() {
            component
                .get_raw("@checks")
                .or_else(|| component.get_raw("checks"))?
        } else {
            component.get_raw(property)?
        };
        if let Some(index) = check {
            value.get(index).cloned()
        } else {
            Some(value.clone())
        }
    }
    fn mount_binding(
        &mut self,
        surface: &str,
        component: &str,
        property: &str,
        base: &str,
        check: Option<usize>,
    ) -> Result<String> {
        if self.mounted.bindings.len() >= 4096 {
            return Err(A2uiError::Validation(
                "mounted binding capacity reached".into(),
            ));
        }
        let source = self
            .mounted_source(surface, component, property, check)
            .ok_or_else(|| {
                A2uiError::Validation("component property or check does not exist".into())
            })?;
        let expression = if check.is_some() {
            source
                .get("condition")
                .cloned()
                .ok_or_else(|| A2uiError::Validation("check requires condition".into()))?
        } else {
            source.clone()
        };
        let evaluation =
            self.evaluator
                .start(surface, base, expression, EvaluationMode::Reactive)?;
        let id = uuid::Uuid::new_v4().to_string();
        self.mounted.bindings.insert(
            id.clone(),
            Binding {
                surface: surface.into(),
                component: component.into(),
                property: property.into(),
                base: base.into(),
                check,
                source,
                evaluation,
            },
        );
        self.advance_evaluations();
        Ok(id)
    }
    pub fn unmount_property(&mut self, id: &str) {
        if let Some(binding) = self.mounted.bindings.remove(id) {
            self.evaluator.forget(&binding.evaluation);
            self.remove_cancelled_evaluation_calls();
        }
    }
    /// Inspect pending/error/cancellation separately from the null value projection.
    pub fn mounted_status(&self, id: &str) -> Option<EvaluationStatus> {
        let binding = self.mounted.bindings.get(id)?;
        self.evaluator.latest_status(&binding.evaluation)
    }
    /// Pending and failed dynamic properties resolve null without affecting siblings.
    pub fn mounted_value(&self, id: &str) -> Option<Value> {
        let binding = self.mounted.bindings.get(id)?;
        let status = self.evaluator.latest_status(&binding.evaluation);
        if binding.check.is_some() {
            let mut result = match status {
                Some(EvaluationStatus::Ready(value))
                    if value.get("valid").and_then(Value::as_bool).is_some() =>
                {
                    value
                }
                Some(EvaluationStatus::Ready(value)) => {
                    json!({"valid":crate::model::data_context::value_to_boolean(&value)})
                }
                Some(EvaluationStatus::Error { .. }) => json!({"valid":false}),
                _ => json!({"valid":true}),
            };
            if result["valid"] == false {
                let object = result.as_object_mut().expect("validation result object");
                object.entry("severity").or_insert_with(|| {
                    binding
                        .source
                        .get("severity")
                        .cloned()
                        .unwrap_or(json!("error"))
                });
                if let Some(message) = binding.source.get("message") {
                    object.entry("message").or_insert_with(|| message.clone());
                }
            }
            return Some(result);
        }
        Some(match status {
            Some(EvaluationStatus::Ready(value)) => value,
            _ => Value::Null,
        })
    }
    pub(super) fn refresh_mounted(&mut self) -> Result<()> {
        let mut bindings = std::mem::take(&mut self.mounted.bindings);
        let mut error = None;
        bindings.retain(|_, binding| {
            let Some(source) = self.mounted_source(
                &binding.surface,
                &binding.component,
                &binding.property,
                binding.check,
            ) else {
                self.evaluator.forget(&binding.evaluation);
                return false;
            };
            if source != binding.source {
                self.evaluator.forget(&binding.evaluation);
                let expression = if binding.check.is_some() {
                    match source.get("condition").cloned() {
                        Some(condition) => condition,
                        None => {
                            error = Some(A2uiError::Validation("check requires condition".into()));
                            return false;
                        }
                    }
                } else {
                    source.clone()
                };
                match self.evaluator.start(
                    &binding.surface,
                    &binding.base,
                    expression,
                    EvaluationMode::Reactive,
                ) {
                    Ok(id) => {
                        binding.evaluation = id;
                        binding.source = source
                    }
                    Err(e) => {
                        error = Some(e);
                        return false;
                    }
                }
            }
            true
        });
        self.mounted.bindings = bindings;
        self.remove_cancelled_evaluation_calls();
        if let Some(error) = error {
            Err(error)
        } else {
            Ok(())
        }
    }
}
