#![forbid(unsafe_code)]
use super::same_axis_move_for;
use crate::settings::{LiveSettings, SAME_AXIS_MOVE_SWAP, Settings};
use tiler_core::directional::SameAxisMove;

#[test]
fn proof_owners_without_live_settings_keep_group_wrap() {
    assert_eq!(same_axis_move_for(None), SameAxisMove::GroupWithNeighbor);
}

#[test]
fn live_setting_selects_subsequent_move_mode() {
    let live = LiveSettings::fresh(Settings::default(), None);
    assert_eq!(
        same_axis_move_for(Some(&live)),
        SameAxisMove::GroupWithNeighbor
    );
    let mut swap = Settings::default();
    swap.core.same_axis_move = SAME_AXIS_MOVE_SWAP.to_owned();
    let live = LiveSettings::fresh(swap, None);
    assert_eq!(
        same_axis_move_for(Some(&live)),
        SameAxisMove::SwapWithNeighbor
    );
}

