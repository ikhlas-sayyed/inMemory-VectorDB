use std::collections::BinaryHeap;

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use serde::{Deserialize, Serialize};

use super::candidate::{Candidate, MaxCandidate, MinCandidate, sort_candidates};
use super::config::Config;
use super::distance::{dot, normalize};
use super::error::{HnswError, Result};
use super::node::Node;

pub type VectorId = usize;

const MAX_LEVEL: usize = 32;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SearchResult {
    pub id: VectorId,
    pub data: Vec<u8>,
    /// Cosine similarity in [-1, 1].
    pub similarity: f32,
}

#[derive(Debug)]
pub struct Hnsw {
    config: Config,
    dim: Option<usize>,
    /// Unit-normalized vectors, so distance = 1 - dot.
    vectors: Vec<Vec<f32>>,
    pub data: Vec<Vec<u8>>,
    nodes: Vec<Node>,
    entry_point: Option<VectorId>,
    max_level: usize,
    deleted_count: usize,
    rng: StdRng,
}

impl Hnsw {
    pub fn new(config: Config) -> Result<Self> {
        config.validate()?;
        let rng = match config.seed {
            Some(seed) => StdRng::seed_from_u64(seed),
            None => StdRng::from_rng(&mut rand::rng()),
        };
        Ok(Self {
            config,
            dim: None,
            data: Vec::new(),
            vectors: Vec::new(),
            nodes: Vec::new(),
            entry_point: None,
            max_level: 0,
            deleted_count: 0,
            rng,
        })
    }

    pub fn with_params(m: usize, ef_construction: usize) -> Result<Self> {
        Self::new(Config::new(m, ef_construction))
    }

    // ---------------- Accessors ----------------

    /// Number of live (not deleted) vectors.
    pub fn len(&self) -> usize {
        self.vectors.len() - self.deleted_count
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn dim(&self) -> Option<usize> {
        self.dim
    }

    pub fn contains(&self, id: VectorId) -> bool {
        self.nodes.get(id).is_some_and(|n| !n.deleted)
    }

    /// Stored (unit-normalized) vector for a live ID.
    pub fn get(&self, id: VectorId) -> Option<&[f32]> {
        if self.contains(id) {
            Some(&self.vectors[id])
        } else {
            None
        }
    }

    // ---------------- Internals ----------------

    fn dist(&self, query: &[f32], id: VectorId) -> f32 {
        1.0 - dot(query, &self.vectors[id])
    }

    fn random_level(&mut self) -> usize {
        // u in (0, 1], so ln(u) is finite.
        let u = 1.0 - self.rng.random::<f64>();
        let level = (-u.ln() * self.config.level_mult()).floor() as usize;
        level.min(MAX_LEVEL)
    }

    /// Validate, check dimension and normalize a query.
    fn prepare_query(&self, query: &[f32]) -> Result<Vec<f32>> {
        let mut q = query.to_vec();
        normalize(&mut q)?;
        if let Some(expected) = self.dim {
            if q.len() != expected {
                return Err(HnswError::DimensionMismatch {
                    expected,
                    got: q.len(),
                });
            }
        }
        Ok(q)
    }

    /// Walk to the closest reachable node on one layer.
    fn greedy_search_layer(&self, query: &[f32], entry: VectorId, layer: usize) -> VectorId {
        let mut current = entry;
        let mut current_dist = self.dist(query, current);

        loop {
            let mut next = current;
            let mut next_dist = current_dist;

            for &n in &self.nodes[current].neighbors[layer] {
                let d = self.dist(query, n);
                if d < next_dist {
                    next_dist = d;
                    next = n;
                }
            }

            if next == current {
                return current;
            }
            current = next;
            current_dist = next_dist;
        }
    }

    /// Best-first search on one layer. Returns up to `ef` candidates, closest first.
    fn search_layer(
        &self,
        query: &[f32],
        entry: VectorId,
        layer: usize,
        ef: usize,
    ) -> Vec<Candidate> {
        let ef = ef.max(1);
        let start = Candidate {
            distance: self.dist(query, entry),
            id: entry,
        };

        let mut candidates = BinaryHeap::new();
        let mut results = BinaryHeap::new();
        candidates.push(MinCandidate(start));
        results.push(MaxCandidate(start));

        let mut visited = vec![false; self.nodes.len()];
        visited[entry] = true;

        while let Some(MinCandidate(current)) = candidates.pop() {
            let worst = results.peek().map_or(f32::INFINITY, |c| c.0.distance);
            if results.len() >= ef && current.distance > worst {
                break;
            }

            for &n in &self.nodes[current.id].neighbors[layer] {
                if std::mem::replace(&mut visited[n], true) {
                    continue;
                }

                let distance = self.dist(query, n);
                let worst = results.peek().map_or(f32::INFINITY, |c| c.0.distance);

                if results.len() < ef || distance < worst {
                    let c = Candidate { distance, id: n };
                    candidates.push(MinCandidate(c));
                    results.push(MaxCandidate(c));
                    if results.len() > ef {
                        results.pop();
                    }
                }
            }
        }

        let mut out: Vec<Candidate> = results.into_iter().map(|c| c.0).collect();
        sort_candidates(&mut out);
        out
    }

    /// Pick up to `m` neighbors from candidates (sorted closest first).
    ///
    /// Heuristic from the paper: skip a candidate if it is closer to an
    /// already-picked neighbor than to the query, then backfill if short.
    fn select_neighbors(&self, candidates: &[Candidate], m: usize) -> Vec<VectorId> {
        let mut selected: Vec<Candidate> = Vec::with_capacity(m);
        let mut skipped: Vec<Candidate> = Vec::new();

        for &c in candidates {
            if selected.len() >= m {
                break;
            }
            let diverse = selected.iter().all(|s| {
                let d = 1.0 - dot(&self.vectors[c.id], &self.vectors[s.id]);
                c.distance < d
            });
            if diverse {
                selected.push(c);
            } else {
                skipped.push(c);
            }
        }

        for c in skipped {
            if selected.len() >= m {
                break;
            }
            selected.push(c);
        }

        selected.into_iter().map(|c| c.id).collect()
    }

    /// Shrink a node's neighbor list back to the layer limit.
    fn prune_neighbors(&mut self, id: VectorId, layer: usize) {
        let max = self.config.max_neighbors(layer);
        if self.nodes[id].neighbors[layer].len() <= max {
            return;
        }

        let mut candidates: Vec<Candidate> = self.nodes[id].neighbors[layer]
            .iter()
            .map(|&n| Candidate {
                distance: 1.0 - dot(&self.vectors[id], &self.vectors[n]),
                id: n,
            })
            .collect();
        sort_candidates(&mut candidates);

        let kept = self.select_neighbors(&candidates, max);
        self.nodes[id].neighbors[layer] = kept;
    }

    // ---------------- Public API ----------------

    /// Insert a vector and return its ID.
    ///
    /// Fails on empty, non-finite, zero-norm or wrong-dimension vectors.
    /// A failed insert leaves the index unchanged.
    pub fn insert(&mut self, vector: Vec<f32>, data: Vec<u8>) -> Result<VectorId> {
        let mut vector = vector;
        normalize(&mut vector)?;
        if let Some(expected) = self.dim {
            if vector.len() != expected {
                return Err(HnswError::DimensionMismatch {
                    expected,
                    got: vector.len(),
                });
            }
        }

        let id = self.vectors.len();
        let level = self.random_level();

        self.dim = Some(vector.len());
        self.vectors.push(vector);
        self.nodes.push(Node::new(level));

        let Some(entry) = self.entry_point else {
            self.entry_point = Some(id);
            self.max_level = level;
            self.data.push(data);
            return Ok(id);
        };

        let top = self.max_level;
        let mut current = entry;

        // Descend through layers above the new node's top layer.
        for layer in ((level + 1)..=top).rev() {
            current = self.greedy_search_layer(&self.vectors[id], current, layer);
        }

        // Connect on every layer the new node shares with the graph.
        for layer in (0..=level.min(top)).rev() {
            let candidates = self.search_layer(
                &self.vectors[id],
                current,
                layer,
                self.config.ef_construction,
            );
            let neighbors = self.select_neighbors(&candidates, self.config.m);

            self.nodes[id].neighbors[layer] = neighbors.clone();
            for &n in &neighbors {
                self.nodes[n].neighbors[layer].push(id);
                self.prune_neighbors(n, layer);
            }

            if let Some(closest) = candidates.first() {
                current = closest.id;
            }
        }

        if level > self.max_level {
            self.max_level = level;
            self.entry_point = Some(id);
        }
        self.data.push(data);

        Ok(id)
    }

    /// Soft-delete a vector. It is skipped in results but still used for routing.
    pub fn remove(&mut self, id: VectorId) -> Result<()> {
        match self.nodes.get_mut(id) {
            Some(node) if !node.deleted => {
                node.deleted = true;
                self.data[id].clear();
                self.deleted_count += 1;
                Ok(())
            }
            _ => Err(HnswError::NotFound(id)),
        }
    }

    /// Approximate k-nearest-neighbor search, most similar first.
    pub fn search(&self, query: &[f32], k: usize, ef_search: usize) -> Result<Vec<SearchResult>> {
        let Some(mut current) = self.entry_point else {
            return Ok(Vec::new());
        };
        let q = self.prepare_query(query)?;
        if k == 0 || self.is_empty() {
            return Ok(Vec::new());
        }

        for layer in (1..=self.max_level).rev() {
            current = self.greedy_search_layer(&q, current, layer);
        }

        // Widen ef so deleted nodes can't crowd out live results.
        let ef = ef_search
            .max(k)
            .saturating_add(self.deleted_count)
            .min(self.nodes.len());

        Ok(self
            .search_layer(&q, current, 0, ef)
            .into_iter()
            .filter(|c| !self.nodes[c.id].deleted)
            .take(k)
            .map(|c| SearchResult {
                id: c.id,
                similarity: (1.0 - c.distance).clamp(-1.0, 1.0),
                data: self.data[c.id].clone(),
            })
            .collect())
    }

    /// Exact search by scanning every live vector. Useful for testing recall.
    pub fn brute_force_search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        if self.entry_point.is_none() {
            return Ok(Vec::new());
        }
        let q = self.prepare_query(query)?;

        let mut all: Vec<Candidate> = (0..self.vectors.len())
            .filter(|&id| !self.nodes[id].deleted)
            .map(|id| Candidate {
                distance: self.dist(&q, id),
                id,
            })
            .collect();
        sort_candidates(&mut all);

        Ok(all
            .into_iter()
            .take(k)
            .map(|c| SearchResult {
                id: c.id,
                similarity: (1.0 - c.distance).clamp(-1.0, 1.0),
                data: self.data[c.id].clone(),
            })
            .collect())
    }
}
