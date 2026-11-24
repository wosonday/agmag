//! Pure layout math for grouping project-board agents into per-CEO column blocks.

/// Minimal input per agent, in display pre-order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    pub depth: usize,
    /// role == "worker"
    pub worker: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    /// index of the CEO in the input
    Ceo(usize),
    NoCeo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    /// levels[0] = the CEO (Ceo) or the orphan roots (NoCeo); levels[k] = indices of that block's agents at relative depth k,
    /// each level in input order.
    pub levels: Vec<Vec<usize>>,
}

/// A CEO is a depth-0 agent that is not a worker.
pub fn is_ceo(node: Node) -> bool {
    node.depth == 0 && !node.worker
}

fn push_at_level(block: &mut Block, depth: usize, idx: usize) {
    if block.levels.len() <= depth {
        block.levels.resize_with(depth.saturating_add(1), Vec::new);
    }
    block.levels[depth].push(idx);
}

/// CEO blocks in input order, then at most one NoCeo block (only when orphans exist) holding every depth-0 worker
/// and their descendants. Every input index appears in exactly one block.
pub fn ceo_blocks(nodes: &[Node]) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut no_ceo: Option<Block> = None;
    let mut current_is_ceo = false;

    for (idx, &node) in nodes.iter().enumerate() {
        if node.depth == 0 {
            if is_ceo(node) {
                blocks.push(Block {
                    kind: BlockKind::Ceo(idx),
                    levels: vec![vec![idx]],
                });
                current_is_ceo = true;
            } else {
                let orphan_block = no_ceo.get_or_insert_with(|| Block {
                    kind: BlockKind::NoCeo,
                    levels: Vec::new(),
                });
                push_at_level(orphan_block, 0, idx);
                current_is_ceo = false;
            }
        } else if current_is_ceo && let Some(ceo_block) = blocks.last_mut() {
            push_at_level(ceo_block, node.depth, idx);
        } else {
            let orphan_block = no_ceo.get_or_insert_with(|| Block {
                kind: BlockKind::NoCeo,
                levels: Vec::new(),
            });
            push_at_level(orphan_block, node.depth, idx);
        }
    }

    if let Some(orphan_block) = no_ceo {
        blocks.push(orphan_block);
    }
    blocks
}

/// Width needed by a block: max over levels of n*card_w + (n-1)*gap, at least card_w.
pub fn block_need(block: &Block, card_w: u16, gap: u16) -> u16 {
    block
        .levels
        .iter()
        .filter(|lvl| !lvl.is_empty())
        .map(|lvl| {
            let n = u16::try_from(lvl.len()).unwrap_or(u16::MAX);
            n.saturating_mul(card_w)
                .saturating_add(n.saturating_sub(1).saturating_mul(gap))
        })
        .max()
        .unwrap_or(0)
        .max(card_w)
}

/// Side-by-side widths for `blocks` inside `width` (blocks separated by `gap`): each block gets its need, the leftover is
/// shared in proportion to need (remainder to the last block). None when the needs plus gaps exceed `width` (caller
/// stacks the blocks vertically instead). Empty input → Some(vec![]).
pub fn block_widths(blocks: &[Block], width: u16, card_w: u16, gap: u16) -> Option<Vec<u16>> {
    if blocks.is_empty() {
        return Some(Vec::new());
    }

    let needs: Vec<u16> = blocks.iter().map(|b| block_need(b, card_w, gap)).collect();
    let total_need: u64 = needs.iter().map(|&n| u64::from(n)).sum();
    let total_gaps: u64 = u64::try_from(blocks.len().saturating_sub(1))
        .unwrap_or(u64::MAX)
        .saturating_mul(u64::from(gap));
    let min_total = total_need.saturating_add(total_gaps);

    if min_total > u64::from(width) {
        return None;
    }

    let leftover = u64::from(width).saturating_sub(min_total);
    let last_idx = needs.len().saturating_sub(1);
    let mut distributed: u64 = 0;
    let mut widths = Vec::with_capacity(needs.len());

    for (i, &need) in needs.iter().enumerate() {
        let extra = if i == last_idx {
            leftover.saturating_sub(distributed)
        } else {
            leftover
                .saturating_mul(u64::from(need))
                .checked_div(total_need)
                .unwrap_or(0)
        };
        distributed = distributed.saturating_add(extra);
        widths.push(need.saturating_add(u16::try_from(extra).unwrap_or(u16::MAX)));
    }

    Some(widths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(depth: usize, worker: bool) -> Node {
        Node { depth, worker }
    }

    #[test]
    fn single_ceo_one_block() {
        let nodes = [node(0, false), node(1, true), node(1, true)];
        assert!(is_ceo(nodes[0]));
        assert!(!is_ceo(nodes[1]));
        let blocks = ceo_blocks(&nodes);
        assert_eq!(
            blocks,
            vec![Block {
                kind: BlockKind::Ceo(0),
                levels: vec![vec![0], vec![1, 2]],
            }]
        );
    }

    #[test]
    fn two_ceos_two_blocks_in_order() {
        let nodes = [
            node(0, false),
            node(1, true),
            node(1, true),
            node(0, false),
            node(1, true),
        ];
        let blocks = ceo_blocks(&nodes);
        assert_eq!(
            blocks,
            vec![
                Block {
                    kind: BlockKind::Ceo(0),
                    levels: vec![vec![0], vec![1, 2]],
                },
                Block {
                    kind: BlockKind::Ceo(3),
                    levels: vec![vec![3], vec![4]],
                },
            ]
        );
    }

    #[test]
    fn orphan_worker_goes_to_no_ceo_last() {
        let nodes = [node(0, true), node(1, true), node(0, false), node(1, true)];
        assert!(!is_ceo(nodes[0]));
        assert!(is_ceo(nodes[2]));
        let blocks = ceo_blocks(&nodes);
        assert_eq!(
            blocks,
            vec![
                Block {
                    kind: BlockKind::Ceo(2),
                    levels: vec![vec![2], vec![3]],
                },
                Block {
                    kind: BlockKind::NoCeo,
                    levels: vec![vec![0], vec![1]],
                },
            ]
        );
    }

    #[test]
    fn deeper_levels_grouped() {
        let nodes = [
            node(0, false),
            node(1, false),
            node(2, true),
            node(2, true),
            node(1, true),
            node(2, true),
        ];
        let blocks = ceo_blocks(&nodes);
        assert_eq!(
            blocks,
            vec![Block {
                kind: BlockKind::Ceo(0),
                levels: vec![vec![0], vec![1, 4], vec![2, 3, 5]],
            }]
        );
    }

    #[test]
    fn every_index_once() {
        let nodes = [
            node(0, true),
            node(1, true),
            node(0, false),
            node(1, false),
            node(2, true),
            node(0, true),
            node(1, true),
            node(2, true),
            node(0, false),
        ];
        let blocks = ceo_blocks(&nodes);
        let mut seen: Vec<usize> = blocks
            .iter()
            .flat_map(|b| b.levels.iter().flat_map(|lvl| lvl.iter().copied()))
            .collect();
        seen.sort_unstable();
        let expected: Vec<usize> = (0..nodes.len()).collect();
        assert_eq!(seen, expected);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].kind, BlockKind::Ceo(2));
        assert_eq!(blocks[1].kind, BlockKind::Ceo(8));
        assert_eq!(blocks[2].kind, BlockKind::NoCeo);
        assert_eq!(blocks[2].levels, vec![vec![0, 5], vec![1, 6], vec![7]]);
    }

    #[test]
    fn block_need_math() {
        let b1 = Block {
            kind: BlockKind::Ceo(0),
            levels: vec![vec![0], vec![1, 2, 3], vec![4, 5]],
        };
        // level 1 has 3 cards: 3*20 + 2*2 = 64
        assert_eq!(block_need(&b1, 20, 2), 64);

        let b_single = Block {
            kind: BlockKind::Ceo(0),
            levels: vec![vec![0]],
        };
        assert_eq!(block_need(&b_single, 20, 2), 20);

        let b_empty = Block {
            kind: BlockKind::NoCeo,
            levels: vec![],
        };
        assert_eq!(block_need(&b_empty, 20, 2), 20);
    }

    #[test]
    fn widths_share_leftover_proportionally() {
        let blocks = vec![
            Block {
                kind: BlockKind::Ceo(0),
                levels: vec![vec![0]], // need = 20
            },
            Block {
                kind: BlockKind::Ceo(1),
                levels: vec![vec![1], vec![2, 3]], // need = 2*20 + 2 = 42
            },
        ];
        // total_need = 62, gap = 2, min_total = 64. For width = 95, leftover = 31.
        // block 0 gets 31 * 20 / 62 = 10 -> 30
        // block 1 gets 31 - 10 = 21 -> 63
        assert_eq!(block_widths(&blocks, 95, 20, 2), Some(vec![30, 63]));

        // Remainder goes to last block: 3 equal blocks of need 20, gap 2, width 66 -> leftover 2.
        let equal_blocks = vec![
            Block {
                kind: BlockKind::Ceo(0),
                levels: vec![vec![0]],
            },
            Block {
                kind: BlockKind::Ceo(1),
                levels: vec![vec![1]],
            },
            Block {
                kind: BlockKind::Ceo(2),
                levels: vec![vec![2]],
            },
        ];
        assert_eq!(
            block_widths(&equal_blocks, 66, 20, 2),
            Some(vec![20, 20, 22])
        );
    }

    #[test]
    fn widths_none_when_too_narrow() {
        let blocks = vec![
            Block {
                kind: BlockKind::Ceo(0),
                levels: vec![vec![0]], // need = 20
            },
            Block {
                kind: BlockKind::Ceo(1),
                levels: vec![vec![1]], // need = 20
            },
        ];
        // min_total = 20 + 20 + 2 = 42
        assert_eq!(block_widths(&blocks, 41, 20, 2), None);
        assert_eq!(block_widths(&blocks, 42, 20, 2), Some(vec![20, 20]));
    }

    #[test]
    fn empty_input() {
        assert_eq!(ceo_blocks(&[]), vec![]);
        assert_eq!(block_widths(&[], 80, 20, 2), Some(vec![]));
    }
}
