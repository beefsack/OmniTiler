/// Choose one increasing baseline index per row. Each row has one column per
/// entry in `removable`; `None` forbids placement, and `false` forbids skipping.
pub(super) fn choose(
    costs: &[Vec<Option<(usize, usize)>>],
    removable: &[bool],
) -> Option<Vec<usize>> {
    let rows = costs.len();
    let columns = removable.len();
    if rows == 0 {
        return removable.iter().all(|&can_skip| can_skip).then(Vec::new);
    }
    // O(rows * columns) time and decision storage, with one score column.
    if rows > columns {
        return None;
    }

    // Suffix scores for the next column, updated in row order so the score
    // for the next row still refers to that column. Widen before summing.
    let mut scores = vec![None; rows + 1];
    scores[rows] = Some((0u128, 0u128));
    let mut take = vec![false; rows * columns];
    for (column, &can_skip) in removable.iter().enumerate().rev() {
        for (row, costs) in costs.iter().enumerate() {
            let place = costs[column].zip(scores[row + 1]).map(
                |((members, views), (remaining_members, remaining_views))| {
                    (
                        members as u128 + remaining_members,
                        views as u128 + remaining_views,
                    )
                },
            );
            let skip = if can_skip { scores[row] } else { None };
            // With equal scores, taking this column makes the first index
            // strictly earlier than it could be after skipping it.
            if let Some(place) = place
                && skip.is_none_or(|skip| place <= skip)
            {
                scores[row] = Some(place);
                take[column * rows + row] = true;
            } else {
                scores[row] = skip;
            }
        }
        if !can_skip {
            scores[rows] = None;
        }
    }

    scores[0]?;
    let mut selected = Vec::with_capacity(rows);
    for (column, decisions) in take.chunks_exact(rows).enumerate() {
        if decisions[selected.len()] {
            selected.push(column);
            if selected.len() == rows {
                break;
            }
        }
    }
    Some(selected)
}

#[cfg(test)]
mod tests {
    use super::choose;

    #[test]
    fn minimizes_total_membership_before_view_writes() {
        let costs = [
            vec![Some((1, 8)), Some((0, 0)), None],
            vec![None, Some((0, 8)), Some((4, 0))],
        ];
        assert_eq!(choose(&costs, &[true; 3]), Some(vec![0, 1]));
    }

    #[test]
    fn minimizes_view_writes_then_chooses_earliest_indices() {
        let costs = [
            vec![Some((0, 0)), Some((0, 0)), None, None],
            vec![None, Some((0, 1)), Some((0, 0)), Some((0, 0))],
        ];
        assert_eq!(choose(&costs, &[true; 4]), Some(vec![0, 2]));
    }

    #[test]
    fn must_retain_nonremovable_columns_even_after_cheaper_candidates() {
        let costs = [vec![Some((0, 0)), None, Some((2, 1))]];
        assert_eq!(choose(&costs, &[true, true, false]), Some(vec![2]));
        assert_eq!(choose(&costs, &[true, false, true]), None);
        assert_eq!(choose(&costs, &[false, true, false]), None);
    }

    #[test]
    fn rejects_placements_that_cannot_be_ordered() {
        let costs = [vec![None, Some((0, 0))], vec![Some((0, 0)), None]];
        assert_eq!(choose(&costs, &[true; 2]), None);
        assert_eq!(choose(&costs, &[true]), None);
    }

    #[test]
    fn handles_empty_selection() {
        assert_eq!(choose(&[], &[]), Some(vec![]));
        assert_eq!(choose(&[], &[true, true]), Some(vec![]));
        assert_eq!(choose(&[], &[true, false]), None);
    }

    #[test]
    fn compares_totals_larger_than_usize() {
        let costs = [
            vec![Some((usize::MAX, 0)), None, None],
            vec![None, Some((2, 0)), Some((1, 0))],
        ];
        assert_eq!(choose(&costs, &[true; 3]), Some(vec![0, 2]));
    }
}
