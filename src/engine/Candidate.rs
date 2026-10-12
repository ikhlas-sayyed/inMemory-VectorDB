use std::cmp::Ordering;

#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub distance: f32,
    pub id: usize,
}

fn compare(a: &Candidate, b: &Candidate) -> Ordering {
    a.distance.total_cmp(&b.distance).then(a.id.cmp(&b.id))
}

/// Sort ascending by distance (closest first).
pub fn sort_candidates(items: &mut [Candidate]) {
    items.sort_by(compare);
}

/// Heap wrapper: pops the closest candidate first.
#[derive(Clone, Copy, Debug)]
pub struct MinCandidate(pub Candidate);

/// Heap wrapper: pops the farthest candidate first.
#[derive(Clone, Copy, Debug)]
pub struct MaxCandidate(pub Candidate);

impl PartialEq for MinCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for MinCandidate {}
impl PartialOrd for MinCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for MinCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        compare(&other.0, &self.0)
    }
}

impl PartialEq for MaxCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for MaxCandidate {}
impl PartialOrd for MaxCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for MaxCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        compare(&self.0, &other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BinaryHeap;

    fn c(distance: f32, id: usize) -> Candidate {
        Candidate { distance, id }
    }

    #[test]
    fn min_heap_pops_closest() {
        let mut h = BinaryHeap::new();
        for (d, i) in [(0.5, 1), (0.1, 2), (0.9, 3)] {
            h.push(MinCandidate(c(d, i)));
        }
        assert_eq!(h.pop().unwrap().0.id, 2);
    }

    #[test]
    fn max_heap_pops_farthest() {
        let mut h = BinaryHeap::new();
        for (d, i) in [(0.5, 1), (0.1, 2), (0.9, 3)] {
            h.push(MaxCandidate(c(d, i)));
        }
        assert_eq!(h.pop().unwrap().0.id, 3);
    }

    #[test]
    fn nan_does_not_panic() {
        let mut v = vec![c(f32::NAN, 0), c(0.2, 1)];
        sort_candidates(&mut v);
        assert_eq!(v.len(), 2);
    }
}
