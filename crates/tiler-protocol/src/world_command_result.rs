//! Canonical JSON writer for correlated [`CommandResult`] settlement
//! outcomes owned by the protocol crate.

use tiler_core::directional::Direction;
use tiler_core::geometry::Rect;
use tiler_core::world_report::CommandResult;

use crate::world_protocol::push_json_string;

/// Exact canonical encoding for the correlated `command_result` section:
/// `null` when absent, else the per-op object with frozen canonical key
/// order.
pub fn encode_command_result(out: &mut String, result: &Option<CommandResult>) {
    use CommandResult as Result;
    let Some(result) = result else {
        out.push_str("null");
        return;
    };
    let push_rect = |out: &mut String, r: Rect| {
        out.push_str(&format!("[{},{},{},{}]", r.x, r.y, r.w, r.h));
    };
    match result {
        Result::Float {
            window,
            rect,
            no_space,
        } => {
            out.push_str("{\"op\":\"toggle-float\",\"win\":");
            push_json_string(out, &window.0);
            out.push_str(",\"rect\":");
            match rect {
                Some(r) => push_rect(out, *r),
                None => out.push_str("null"),
            }
            // Explicit-tile no-space retry marker: present only when true
            // so older successful placements stay byte-identical.
            if *no_space {
                out.push_str(",\"noSpace\":true");
            }
            out.push('}');
        }
        Result::Migration {
            direction,
            source,
            target,
            members,
            floats,
        } => {
            out.push_str("{\"op\":\"migrate-workspace\",\"direction\":");
            push_json_string(
                out,
                match direction {
                    Direction::Left => "left",
                    Direction::Right => "right",
                    Direction::Up => "up",
                    Direction::Down => "down",
                },
            );
            out.push_str(",\"sourceOut\":");
            push_json_string(out, &source.output.0);
            out.push_str(",\"sourceWs\":");
            push_json_string(out, &source.workspace.0);
            out.push_str(",\"targetOut\":");
            push_json_string(out, &target.output.0);
            out.push_str(",\"targetWs\":");
            push_json_string(out, &target.workspace.0);
            out.push_str(",\"members\":[");
            for (i, member) in members.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json_string(out, &member.0);
            }
            out.push_str("],\"floats\":");
            out.push_str(&floats.to_string());
            out.push('}');
        }
        Result::Directional {
            outcome,
            target,
            rect,
            cause,
        } => {
            out.push_str("{\"op\":\"directional\",\"outcome\":");
            push_json_string(out, outcome.as_str());
            out.push_str(",\"win\":");
            match target {
                Some(t) => push_json_string(out, &t.0),
                None => out.push_str("null"),
            }
            out.push_str(",\"rect\":");
            match rect {
                Some(r) => push_rect(out, *r),
                None => out.push_str("null"),
            }
            out.push_str(",\"cause\":");
            match cause {
                Some(c) => push_json_string(out, c),
                None => out.push_str("null"),
            }
            out.push('}');
        }
        Result::Maximize {
            window,
            maximized,
            clear_fullscreen,
            was_maximized,
        } => {
            out.push_str("{\"op\":\"maximize\",\"win\":");
            push_json_string(out, &window.0);
            out.push_str(",\"maximized\":");
            out.push_str(if *maximized { "true" } else { "false" });
            out.push_str(",\"clearFullscreen\":");
            out.push_str(if *clear_fullscreen { "true" } else { "false" });
            out.push_str(",\"wasMaximized\":");
            out.push_str(if *was_maximized { "true" } else { "false" });
            out.push('}');
        }
        Result::Rejected { kind } => {
            out.push_str("{\"op\":\"rejected\",\"kind\":");
            push_json_string(out, kind.as_str());
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
    use tiler_core::session::DomainKey;
    use tiler_core::world_directional::DirectionalOutcome;
    use tiler_core::world_report::CommandRejectKind;

    #[test]
    fn encodes_all_command_result_arms_and_escaping() {
        let encode = |r: Option<CommandResult>| {
            let mut out = String::new();
            encode_command_result(&mut out, &r);
            out
        };

        assert_eq!(encode(None), "null");

        assert_eq!(
            encode(Some(CommandResult::Float {
                window: WindowId("w\"1\\a\x01".into()),
                rect: Some(Rect {
                    x: 1,
                    y: 2,
                    w: 3,
                    h: 4
                }),
                no_space: true,
            })),
            r#"{"op":"toggle-float","win":"w\"1\\a\u0001","rect":[1,2,3,4],"noSpace":true}"#
        );

        assert_eq!(
            encode(Some(CommandResult::Float {
                window: WindowId("w1".into()),
                rect: None,
                no_space: false,
            })),
            r#"{"op":"toggle-float","win":"w1","rect":null}"#
        );

        assert_eq!(
            encode(Some(CommandResult::Migration {
                direction: Direction::Right,
                source: DomainKey {
                    output: OutputId("out-1".into()),
                    workspace: WorkspaceId("ws-1".into()),
                },
                target: DomainKey {
                    output: OutputId("out-2".into()),
                    workspace: WorkspaceId("ws-2".into()),
                },
                members: vec![WindowId("w1".into()), WindowId("w2".into())],
                floats: 1,
            })),
            r#"{"op":"migrate-workspace","direction":"right","sourceOut":"out-1","sourceWs":"ws-1","targetOut":"out-2","targetWs":"ws-2","members":["w1","w2"],"floats":1}"#
        );

        assert_eq!(
            encode(Some(CommandResult::Directional {
                outcome: DirectionalOutcome::MoveFloatApplied,
                target: Some(WindowId("w1".into())),
                rect: Some(Rect {
                    x: 10,
                    y: 20,
                    w: 30,
                    h: 40
                }),
                cause: Some("ok".into()),
            })),
            r#"{"op":"directional","outcome":"move-float-applied","win":"w1","rect":[10,20,30,40],"cause":"ok"}"#
        );

        assert_eq!(
            encode(Some(CommandResult::Maximize {
                window: WindowId("win-a".into()),
                maximized: true,
                clear_fullscreen: false,
                was_maximized: false,
            })),
            r#"{"op":"maximize","win":"win-a","maximized":true,"clearFullscreen":false,"wasMaximized":false}"#
        );

        assert_eq!(
            encode(Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused,
            })),
            r#"{"op":"rejected","kind":"operation-refused"}"#
        );
    }
}
