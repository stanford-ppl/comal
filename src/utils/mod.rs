use rand_distr::Distribution;

/// This stores the conceptual CSF format, which is generally easier to manipulate and reason about.
pub enum SparseTree<CoordType, ValType> {
    Outer(Vec<(CoordType, SparseTree<CoordType, ValType>)>),
    Inner(Vec<(CoordType, ValType)>),
}

impl<CT, VT> SparseTree<CT, VT> {
    /// Only checks if the topmost level is empty.
    pub fn is_empty_top(&self) -> bool {
        match self {
            SparseTree::Outer(x) => x.is_empty(),
            SparseTree::Inner(x) => x.is_empty(),
        }
    }

    /// Returns the number of nonzero in this dimension only
    pub fn num_nonzero(&self) -> usize {
        match self {
            SparseTree::Outer(x) => x.len(),
            SparseTree::Inner(x) => x.len(),
        }
    }
}

impl<CT: Clone, VT: Clone> SparseTree<CT, VT> {
    pub fn to_coo(&self) -> Vec<(Vec<CT>, VT)> {
        self.coo_helper()
            .into_iter()
            .map(|(mut crd, v)| {
                crd.reverse();
                (crd, v)
            })
            .collect()
    }

    fn coo_helper(&self) -> Vec<(Vec<CT>, VT)> {
        match self {
            SparseTree::Outer(children) => children
                .iter()
                .flat_map(|(coord, subtree)| {
                    let mut sub = subtree.coo_helper();
                    sub.iter_mut().for_each(|(crd, _)| crd.push(coord.clone()));
                    sub
                })
                .collect(),
            SparseTree::Inner(pairs) => pairs
                .iter()
                .map(|(crd, val)| (vec![crd.clone()], val.clone()))
                .collect(),
        }
    }

    pub fn compute_rank(&self) -> Option<usize> {
        match self {
            SparseTree::Outer(nested) => {
                let mut sub_rank = None;
                for (_, subtree) in nested {
                    let inner_rank = subtree.compute_rank();
                    match (sub_rank, inner_rank) {
                        // Subtree had undefined rank, so this has undefined rank.
                        (_, None) => {
                            return None;
                        }

                        // Subtree had a rank, but it doesn't agree with the rank we have so far
                        (Some(a), Some(b)) if a != b => {
                            return None;
                        }

                        // Subtree has a rank, but we don't have one yet.
                        (None, Some(x)) => {
                            sub_rank = Some(x);
                        }

                        // Subtree has a rank and agrees with the current rank.
                        _ => {}
                    }
                }
                // Bump the rank up by 1 to account for our new level of wrapping.
                sub_rank.map(|x| x + 1)
            }
            SparseTree::Inner(_) => Some(1),
        }
    }

    pub fn to_csf(&self) -> CompressedSparseFiber<CT, VT> {
        let mut csf = CompressedSparseFiber {
            outer_levels: {
                let num_outer_ranks = self
                    .compute_rank()
                    .expect("Attempted to convert an ill-formed tensor into CSF")
                    - 1;
                // Push in a level per rank
                (0..num_outer_ranks)
                    .map(|_| Level {
                        ids: vec![],
                        payload: vec![0],
                    })
                    .collect()
            },
            inner_level: Level {
                ids: vec![],
                payload: vec![],
            },
        };
        self.csf_helper(&mut csf, 0);
        csf
    }

    fn csf_helper(&self, workspace: &mut CompressedSparseFiber<CT, VT>, depth: usize) {
        match self {
            SparseTree::Outer(subtrees) => {
                for (crd, subtree) in subtrees {
                    subtree.csf_helper(workspace, depth + 1);

                    let next_level_size = match subtree {
                        SparseTree::Outer(_) => workspace.outer_levels[depth + 1].ids.len(),
                        SparseTree::Inner(_) => workspace.inner_level.ids.len(),
                    };
                    let current_level = &mut workspace.outer_levels[depth];
                    current_level.ids.push(crd.clone());
                    current_level.payload.push(next_level_size);
                }
            }
            SparseTree::Inner(coord_val_pairs) => {
                workspace
                    .inner_level
                    .payload
                    .extend(coord_val_pairs.iter().map(|(_, val)| val.clone()));
                workspace
                    .inner_level
                    .ids
                    .extend(coord_val_pairs.iter().map(|(crd, _)| crd.clone()));
            }
        }
    }
}

impl<CT: TryFrom<usize>, VT> SparseTree<CT, VT>
where
    <CT as TryFrom<usize>>::Error: std::fmt::Debug,
{
    pub fn random(
        shape: &[usize],
        prob_nonzero: f64,
        rng: &mut impl rand::Rng,
        value_distribution: &impl Distribution<VT>,
    ) -> Self {
        let mut probabilities = vec![prob_nonzero];
        for dim in shape[..(shape.len() - 1)].iter().rev() {
            let cur_prob = *probabilities.last().unwrap();
            let new_probability = 1.0 - (1.0 - cur_prob).powi(*dim as i32);
            probabilities.push(new_probability);
        }
        probabilities.reverse();
        Self::random_helper(shape, &probabilities, rng, value_distribution)
    }

    fn random_helper(
        shape: &[usize],
        prob_nonzero: &[f64],
        rng: &mut impl rand::Rng,
        value_distribution: &impl Distribution<VT>,
    ) -> Self {
        assert!(shape.len() > 0);
        let current_size = shape[0];
        let current_prob = prob_nonzero[0];
        if shape.len() == 1 {
            let mut data = vec![];
            for i in 0..current_size {
                if rng.gen_bool(current_prob) {
                    data.push((CT::try_from(i).unwrap(), value_distribution.sample(rng)))
                }
            }

            Self::Inner(data)
        } else {
            let mut data = vec![];
            for i in 0..current_size {
                if rng.gen_bool(current_prob) {
                    let index = CT::try_from(i).unwrap();
                    let subtree = Self::random_helper(
                        &shape[1..],
                        &prob_nonzero[1..],
                        rng,
                        value_distribution,
                    );
                    if !subtree.is_empty_top() {
                        data.push((index, subtree))
                    }
                }
            }
            Self::Outer(data)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct CompressedSparseFiber<CoordType, ValType> {
    pub outer_levels: Vec<Level<CoordType, usize>>,
    pub inner_level: Level<CoordType, ValType>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Level<IDType, PayloadType> {
    pub ids: Vec<IDType>,
    pub payload: Vec<PayloadType>,
}

#[cfg(test)]
mod tests {
    use crate::utils::{CompressedSparseFiber, Level};

    use super::SparseTree;

    const EXAMPLE_TREE: fn() -> SparseTree<i32, f64> = || {
        SparseTree::Outer(vec![
            (
                1,
                SparseTree::Outer(vec![
                    (
                        1,
                        SparseTree::Outer(vec![(1, SparseTree::Inner(vec![(2, 1.0), (3, 2.0)]))]),
                    ),
                    (
                        2,
                        SparseTree::Outer(vec![
                            (1, SparseTree::Inner(vec![(1, 3.0), (3, 4.0)])),
                            (2, SparseTree::Inner(vec![(1, 5.0)])),
                        ]),
                    ),
                ]),
            ),
            (
                2,
                SparseTree::Outer(vec![(
                    2,
                    SparseTree::Outer(vec![(
                        2,
                        SparseTree::Inner(vec![(1, 6.0), (2, 7.0), (3, 8.0)]),
                    )]),
                )]),
            ),
        ])
    };

    #[test]
    fn test_sparsetree_to_coo() {
        let st = EXAMPLE_TREE();

        let gold = vec![
            (vec![1, 1, 1, 2], 1.0),
            (vec![1, 1, 1, 3], 2.0),
            (vec![1, 2, 1, 1], 3.0),
            (vec![1, 2, 1, 3], 4.0),
            (vec![1, 2, 2, 1], 5.0),
            (vec![2, 2, 2, 1], 6.0),
            (vec![2, 2, 2, 2], 7.0),
            (vec![2, 2, 2, 3], 8.0),
        ];
        assert_eq!(st.to_coo(), gold);
        assert_eq!(st.compute_rank(), Some(4));
    }

    #[test]
    fn test_sparsetree_to_csf() {
        let st = EXAMPLE_TREE();
        let csf = st.to_csf();
        let gold = CompressedSparseFiber {
            outer_levels: vec![
                Level {
                    ids: vec![1, 2],
                    payload: vec![0, 2, 3],
                },
                Level {
                    ids: vec![1, 2, 2],
                    payload: vec![0, 1, 3, 4],
                },
                Level {
                    ids: vec![1, 1, 2, 2],
                    payload: vec![0, 2, 4, 5, 8],
                },
            ],
            inner_level: Level {
                ids: vec![2, 3, 1, 3, 1, 1, 2, 3],
                payload: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
            },
        };
        assert_eq!(csf, gold);
    }

    #[test]
    fn test_random() {
        use rand::SeedableRng;
        let st = SparseTree::<i32, f64>::random(
            &[8, 8, 512, 64],
            0.4,
            &mut rand::rngs::StdRng::seed_from_u64(42),
            &rand::distributions::Uniform::new(0.0, 100.0),
        );
        assert_eq!(st.compute_rank(), Some(4));
        let coo = st.to_coo();
        dbg!(coo.len());
    }
}
