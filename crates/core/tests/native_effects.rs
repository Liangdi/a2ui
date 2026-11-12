use a2ui_base::{
    catalog::{
        Catalog,
        function_api::{DataEffect, FunctionImplementation, ReturnType},
    },
    error::A2uiError,
    evaluation::{EvaluationMode, EvaluationStatus},
    message_processor::MessageProcessor,
    model::data_context::DataContext,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
struct Write {
    count: Arc<AtomicUsize>,
    bad: bool,
    fail: bool,
    wrong_return: bool,
}
impl FunctionImplementation for Write {
    fn name(&self) -> &'static str {
        "write"
    }
    fn return_type(&self) -> ReturnType {
        ReturnType::Boolean
    }
    fn allowed_callers(&self) -> a2ui_base::catalog::function_api::AllowedCallers {
        a2ui_base::catalog::function_api::AllowedCallers::RendererOrAgent
    }
    fn data_effects(
        &self,
        args: &HashMap<String, Value>,
        _: &DataContext,
    ) -> Result<Vec<DataEffect>, A2uiError> {
        let mut effects = vec![DataEffect::Set {
            path: args
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("/done")
                .into(),
            value: json!(true),
        }];
        if self.bad {
            effects.push(DataEffect::Set {
                path: "/original/child".into(),
                value: json!(true),
            });
        }
        Ok(effects)
    }
    fn execute(&self, _: &HashMap<String, Value>, _: &DataContext) -> Result<Value, A2uiError> {
        self.count.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(A2uiError::Validation("failed".into()));
        }
        Ok(if self.wrong_return {
            json!("not boolean")
        } else {
            json!(true)
        })
    }
}
fn setup(bad: bool, fail: bool, wrong_return: bool) -> (MessageProcessor, Arc<AtomicUsize>) {
    let count = Arc::new(AtomicUsize::new(0));
    let mut p = MessageProcessor::new(vec![Catalog::new("c").with_function(Box::new(Write {
        count: count.clone(),
        bad,
        fail,
        wrong_return,
    }))]);
    send(
        &mut p,
        json!({"version":"1.0","createSurface":{"surfaceId":"s","catalogId":"c","dataModel":{"original":1}}}),
    );
    (p, count)
}
fn send(p: &mut MessageProcessor, v: Value) {
    p.process_json_message(&v.to_string()).unwrap();
}
fn model(p: &MessageProcessor) -> Value {
    p.model
        .get_surface("s")
        .unwrap()
        .data_model
        .borrow()
        .as_value()
        .clone()
}
fn expression() -> Value {
    json!({"@call":"write","args":{}})
}
#[test]
fn effects_are_atomic_and_execution_errors_do_not_commit() {
    for (bad, fail, wrong) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let (mut p, count) = setup(bad, fail, wrong);
        let id = p
            .start_evaluation("s", "", expression(), EvaluationMode::Action)
            .unwrap();
        assert!(matches!(
            p.evaluation_status(&id),
            Some(EvaluationStatus::Error { .. })
        ));
        assert_eq!(model(&p), json!({"original":1}));
        assert_eq!(count.load(Ordering::SeqCst), usize::from(!bad));
    }
}
#[test]
fn passive_and_synchronous_calls_cannot_apply_native_effects() {
    let (mut p, count) = setup(false, false, false);
    let id = p
        .start_evaluation("s", "", expression(), EvaluationMode::Reactive)
        .unwrap();
    assert!(matches!(
        p.evaluation_status(&id),
        Some(EvaluationStatus::Error { .. })
    ));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(model(&p), json!({"original":1}));
    let surface = p.model.get_surface("s").unwrap();
    let data = surface.data_model.borrow();
    let catalog = p.catalog("c").unwrap();
    let context = DataContext::new(&data, &catalog.functions)
        .with_current_protocol()
        .with_user_activation();
    assert!(context.resolve_dynamic_json(&expression()).is_err());
}
#[test]
fn ordered_effects_are_visible_and_not_replayed_after_remote_resume() {
    let (mut p, count) = setup(false, false, false);
    let watch = p
        .start_evaluation("s", "", json!({"@path":"/done"}), EvaluationMode::Reactive)
        .unwrap();
    let id = p
        .start_evaluation(
            "s",
            "",
            json!([expression(),{"@call":"remote","args":{"value":{"@path":"/done"}}}]),
            EvaluationMode::Action,
        )
        .unwrap();
    let outgoing = p.drain_outgoing();
    assert_eq!(outgoing.len(), 1);
    let call = serde_json::to_value(&outgoing[0]).unwrap()["callAgentFunction"].clone();
    assert_eq!(call["callFunction"]["args"]["value"], true);
    assert_eq!(
        p.evaluation_status(&watch),
        Some(EvaluationStatus::Ready(json!(true)))
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":call["functionCallId"],"value":"resolved"}}),
    );
    assert_eq!(
        p.evaluation_status(&id),
        Some(EvaluationStatus::Ready(json!([true, "resolved"])))
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
}
#[test]
fn pending_or_failed_predecessor_prevents_later_effect_and_cancel_is_final() {
    let (mut p, count) = setup(false, false, false);
    let id = p
        .start_evaluation(
            "s",
            "",
            json!([{"@call":"remote","args":{}},expression()]),
            EvaluationMode::Action,
        )
        .unwrap();
    let call =
        serde_json::to_value(p.drain_outgoing().pop().unwrap()).unwrap()["callAgentFunction"]
            .clone();
    assert_eq!(count.load(Ordering::SeqCst), 0);
    p.cancel_evaluation(&id);
    send(
        &mut p,
        json!({"version":"1.0","agentFunctionResponse":{"functionCallId":call["functionCallId"],"value":true}}),
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(model(&p), json!({"original":1}));
}
#[test]
fn format_precision_is_bounded_and_open_url_requires_activation() {
    use a2ui_base::catalog::basic_functions::{
        FormatCurrencyFunction, FormatNumberFunction, OpenUrlFunction,
    };
    let data = a2ui_base::model::data_model::DataModel::new();
    let functions = HashMap::new();
    let context = DataContext::new(&data, &functions);
    for decimals in [
        json!(-1),
        json!(0.5),
        json!(101),
        json!(1000000000),
        json!("2"),
    ] {
        let args = HashMap::from([
            ("value".into(), json!(1)),
            ("currency".into(), json!("USD")),
            ("decimals".into(), decimals),
        ]);
        assert!(FormatNumberFunction.execute(&args, &context).is_err());
        assert!(FormatCurrencyFunction.execute(&args, &context).is_err());
    }
    assert!(OpenUrlFunction.requires_user_activation());
}
#[test]
fn relative_effects_stay_in_current_surface_template_scope() {
    let (mut p, _) = setup(false, false, false);
    send(
        &mut p,
        json!({"version":"1.0","updateDataModel":{"surfaceId":"s","path":"/rows","value":[{},{}]}}),
    );
    send(
        &mut p,
        json!({"version":"1.0","createSurface":{"surfaceId":"other","catalogId":"c","dataModel":{"rows":[{},{}]}}}),
    );
    p.start_evaluation(
        "s",
        "/rows/1",
        json!({"@call":"write","args":{"path":"done"}}),
        EvaluationMode::Action,
    )
    .unwrap();
    assert_eq!(model(&p), json!({"original":1,"rows":[{},{"done":true}]}));
    assert_eq!(
        p.model
            .get_surface("other")
            .unwrap()
            .data_model
            .borrow()
            .as_value(),
        &json!({"rows":[{},{}]})
    );
}

#[test]
fn current_update_and_effect_paths_reject_bad_pointer_syntax() {
    let (mut p, count) = setup(false, false, false);
    for path in ["name", "/bad~2escape", "/bad~"] {
        assert!(p.process_json_message(&json!({"version":"1.0","updateDataModel":{"surfaceId":"s","path":path,"value":true}}).to_string()).is_err());
    }
    let id = p
        .start_evaluation(
            "s",
            "",
            json!({"@call":"write","args":{"path":"/bad~2escape"}}),
            EvaluationMode::Action,
        )
        .unwrap();
    assert!(matches!(
        p.evaluation_status(&id),
        Some(EvaluationStatus::Error { .. })
    ));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(model(&p), json!({"original":1}));
}

#[test]
fn incoming_rpc_even_with_host_gesture_cannot_silently_drop_native_effects() {
    let (mut p, count) = setup(false, false, false);
    let call=serde_json::from_value(json!({"surfaceId":"s","functionCallId":"incoming","callFunction":{"@call":"write","catalogId":"c","args":{}}})).unwrap();
    p.process_renderer_call_from_user_gesture(&call).unwrap();
    let response = serde_json::to_value(p.drain_outgoing().pop().unwrap()).unwrap();
    assert_eq!(
        response["rendererFunctionResponse"]["error"]["code"],
        "INVALID_FUNCTION_CALL"
    );
    assert!(
        response["rendererFunctionResponse"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("trusted action processor")
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(model(&p), json!({"original":1}));
}
