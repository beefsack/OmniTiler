use serde_json::{Value, json};
use tiler_protocol::world_protocol::decode_reply_v4;

fn set_pointer(document: &mut Value, pointer: &str, replacement: Option<Value>) {
    let (parent, key) = pointer.rsplit_once('/').expect("non-root pointer");
    let parent = document.pointer_mut(parent).expect("pointer parent");
    if let Some(array) = parent.as_array_mut() {
        let index: usize = key.parse().unwrap();
        match replacement {
            Some(value) if index == array.len() => array.push(value),
            Some(value) => array[index] = value,
            None => {
                array.remove(index);
            }
        }
    } else {
        let object = parent.as_object_mut().expect("object parent");
        match replacement {
            Some(value) => {
                object.insert(key.to_owned(), value);
            }
            None => {
                assert!(object.remove(key).is_some());
            }
        }
    }
}

pub fn produce(id: &str, fresh_source: &str, pointer: &str) -> String {
    let source: Value = serde_json::from_str(fresh_source).unwrap();
    decode_reply_v4(fresh_source).expect("fresh source decodes");
    let malformed = ("malformed", "request is malformed");
    let unknown = ("unknown-value", "request contains an unknown value");
    let extra = ("unknown-field", "request contains an unknown field");
    let (replacement, (kind, message)) = match id {
        "invalid-scope-window-arm" => (Some(json!({"kind": "window", "win": "win-a"})), unknown),
        "invalid-scope-output-extra-ws" => (Some(json!("ws-a")), extra),
        "invalid-workspace-trailing-false" => (
            Some(json!(false)),
            ("snapshot-invalid", "workspace-trailing-false"),
        ),
        "invalid-workspace-null-bind"
        | "invalid-output-null-view"
        | "invalid-hold-null-reason"
        | "invalid-desire-null-frame-op"
        | "invalid-operation-cause-null" => (Some(Value::Null), malformed),
        "invalid-hold-unknown-reason" => (Some(json!("unknown")), unknown),
        "invalid-nonhold-reason" => (
            Some(json!("unreadable")),
            ("snapshot-invalid", "nonhold-reason"),
        ),
        "invalid-show-with-hide-fallback" => (
            Some(json!(true)),
            ("snapshot-invalid", "visibility-show-fallback"),
        ),
        "invalid-result-applied-wrapper" => (
            Some(json!({"op": "applied", "result": source["result"].clone()})),
            unknown,
        ),
        "invalid-result-noop-wrapper" => (Some(json!({"op": "noop"})), unknown),
        "invalid-result-float-false-marker" => (
            Some(json!(false)),
            ("snapshot-invalid", "result-float-false-marker"),
        ),
        "invalid-directional-cause-string-empty" => (Some(json!("")), unknown),
        "invalid-operation-outcome-visibility" => (Some(json!("visibility-complete")), unknown),
        "invalid-border-old-absolute" => (Some(json!([8, 8, 800, 600])), extra),
        "invalid-border-cloaked" => (Some(json!("cloaked")), unknown),
        "invalid-underlay-missing-leaf" | "invalid-close-focus-missing-lane" => (None, malformed),
        "invalid-state-target-count" => {
            let mut targets = source.pointer(pointer).unwrap().as_array().unwrap().clone();
            targets.push(targets[0].clone());
            (
                Some(Value::Array(targets)),
                ("snapshot-invalid", "state-target-count"),
            )
        }
        "invalid-close-focus-unknown-cause" => (Some(json!("unknown")), unknown),
        _ => panic!("missing negative recipe: {id}"),
    };
    let mut invalid = source.clone();
    set_pointer(&mut invalid, pointer, replacement);
    assert_ne!(invalid, source, "{id} mutation changes source");
    let wire = serde_json::to_string(&invalid).unwrap();
    let error = decode_reply_v4(&wire).expect_err("negative refuses");
    assert_eq!((error.kind, error.message), (kind, message), "{id}");
    set_pointer(&mut invalid, pointer, source.pointer(pointer).cloned());
    assert_eq!(
        invalid, source,
        "{id} single-pointer repair restores source"
    );
    decode_reply_v4(&serde_json::to_string(&invalid).unwrap()).expect("repair decodes");
    wire
}
